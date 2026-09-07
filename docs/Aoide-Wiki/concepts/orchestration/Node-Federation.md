---
type: concept
created: 2026-08-15
updated: 2026-09-03
tags: [aoide, agent, a2a, orchestration, interop, federation]
---

# Node Federation — aoide-to-aoide graph folding

Node federation is aoide's fourth door onto itself: one aoide instance
registers ANOTHER aoide instance as a **node** by URL and pulls its resolved
session graph into its own, folded in as a subtree. It is not a new
transport — it is built entirely on top of the existing [[A2A-Door]] (§6):
one new JSON-RPC method (`aoide/graphSummary`), a client-side node registry
plus a per-node pull cache, and an additive fold in `build_graph` (the same
function that produces bare `graph`'s document). Spec: `CONTRACTS.md`
§7 (v0, 2026-08-14).

**Melete-optional** — federation works standalone; nothing in it references
or requires [[Melete]]. A Melete-side consumer (a polling skill, first-class
graph rendering) is separate, independently-owned work, not part of this
contract.

**Topology-blind, same-network only in practice.** A node is addressed by a
plain URL — the protocol itself carries no notion of "same LAN" vs.
"tailnet" vs. "the internet"; any reachable URL works the same way. The
house runs nodes over Tailscale (addressed by tailnet MagicDNS hostname),
but nothing here is tailnet-specific. The integration test
(`pkgs/aoide/crates/cli/tests/node_connectivity.rs`) proves the real
mechanism end to end: two actual `a2a::serve()` instances, a genuine `curl`
AgentCard fetch for `node add`, a genuine `curl` POST of `aoide/graphSummary`
for `node pull`, a genuine cache write, a genuine graph fold. Both binds stay
strictly loopback (`127.0.0.1:<port>`), same-subnet HTTP reachability
standing in for "two boxes on the same network." **WAN /
NAT-traversal / relay reachability for a node that is NOT on the same
network is explicitly out of scope for this v0** — a later, separate
contract amendment, not designed or assumed here. A **paired** node has a
narrower escape hatch past plain HTTP reachability: an explicit `via`
marker reaches the door through a session-scoped ssh tunnel instead of
dialing directly ([[Node-Transport]]) — still not NAT traversal or a
relay, since it requires the same ssh access a trusted network already
assumes, but it does cross a segment boundary a router filters for plain
HTTP.

## The registry — `state/nodes.json`

The set of other aoide instances this one has registered, written atomically
via `aoide_storage::node_store`. Lives in the gitignored root-runtime
`state/` dir — **not** either stage tree (`state/stage/` conducting state or
`song/stage/` rice staging): a node roster is account/global
external-registry state, not stage-scoped rehearsal state. Additive/
tolerate-missing: an absent file just means "no nodes registered," never
an error.

```json
{
  "schemaVersion": "0",
  "nodes": [
    { "name": "yomi-strix", "url": "http://yomi-strix:8710/", "autogate": false, "addedAt": "2026-08-14T00:00:00Z",
      "pubkey": "<hex>", "verified": true, "allows": ["read", "spawn"] }
  ]
}
```

`pubkey`/`verified`/`allows` land when the [[Pairing-Ceremony]] commits the
record; a never-paired (legacy-registered) node carries none of them and
never enters the signature rung's trial set.

`autogate` (default `false`) is the cross-device analogue of `send`'s
local "sender is the target's own parent" autogate rule: a node marked
`true` has its inbound `message/send` auto-deliver without the pending
queue even though the connection is non-loopback — see
[[#Security — pairing is the verification path]] below. An
unmarked/unknown sender is never autogated.

`tokenFile` (`node add --token-file <path>`, optional) names a file on THIS
instance holding the shared secret that node presents as `Authorization:
Bearer <token>` — a per-node credential that identifies WHICH registered
node is calling once address alone can't (behind a proxy or tunnel every
caller's address looks the same). Absent by default: an unmarked node
authenticates by address only.

`bearerSecret` (`node add --bearer-secret <name>`, optional) is the opposite
direction: the name of a secret THIS instance resolves through its own local
[[Secrets-Broker]], as consumer `a2a-client`, and presents as `Authorization:
Bearer <value>` on every OUTBOUND call to that node's A2A door. It is
resolved fresh on every request, never cached in `nodes.json` or anywhere
else, so revoking the underlying secret takes effect on the very next call.
Absent by default: an unmarked node's outbound requests carry no bearer.

## Declared vs. registered — `config.toml`'s `[mesh.<name>]`

This registry is STATE — the product of actually running the
[[Pairing-Ceremony]]. `config.toml`'s `[mesh.<name>]` (task #135 P4,
CONTRACTS.md §4) is the other half, INTENT — an operator's own named
roster of who a box is meant to belong with, written once and rarely
touched, never transmitted, never a field on `Node` above. `aoide mesh`
(`aoide_client::mesh`) is the read-only bridge between the two: it compares
a declaration against this registry and reports a node that's missing,
unverified, or reachable at a different hop than declared, plus — kept
separate, never counted as drift — any verified node belonging to no
declared mesh. It writes neither side.

`aoide mesh pair` (task #135 P5) is what closes that gap, and it closes it
the only way this registry is ever written — by running the ordinary
[[Pairing-Ceremony]] against each declared node this box has no verified
record of, in declared-name order, through that node's declared hop. A
verified node is never modified: a hop that no longer matches is reported
and left for a human re-pair, so a second converge over a converged mesh
does nothing. The declaration is still never transmitted and `Node` still
gains no field for it — a converge mints ordinary pairwise records and
nothing else. See [[Doors-and-Nodes#aoide mesh]] for both commands and
`docs/architecture/PAIRING.md`'s "Mesh declaration" section for why a
declared mesh stays intent rather than a second identity model.

## The cache — `state/node-cache/<name>.json`

One node's last-**pulled** `aoide/graphSummary` response, written by `aoide
node pull [<name>]`. Sibling of `state/nodes.json` — same dir family, same
tolerate-missing discipline (no file means "never pulled").

```json
{
  "schemaVersion": "0",
  "name": "yomi-strix",
  "instance": { "name": "yomi-strix", "url": "http://yomi-strix:8710/", "emittedAt": "2026-08-14T00:05:00Z" },
  "graph": { "schemaVersion": "0", "nodes": [], "edges": [] },
  "fetchedAt": "2026-08-14T00:05:03Z",
  "stale": false,
  "lastError": null
}
```

`instance`/`graph`/`fetchedAt` are the node's own response from its LAST
SUCCESSFUL pull, verbatim — `graph` is that node's own already-resolved
`graph.json` v0 document, unmodified. A **failed** pull (unreachable,
timeout, non-200, malformed body) never deletes this file or clears these
fields: it sets `stale: true` and `lastError` to a short reason, preserving
the last-known-good `instance`/`graph` — one node being down must never
blank it out of the fold, and pulling several nodes must never let one
failure abort the others (each node's outcome is independent).

Freshness is governed by a single named constant,
`aoide_storage::node_store::NODE_CACHE_TTL_SECS` (5 minutes) — not a magic
number re-typed at each call site. An entry is `fresh` when `stale == false`
AND `fetchedAt` is within the TTL of now; otherwise `stale` (covers both an
explicit failure mark and a plain TTL expiry). `aoide node status` and the
graph fold below share this exact classification, so they can never
disagree.

## `aoide/graphSummary` — the one new wire method

A single new method on the EXISTING A2A JSON-RPC/HTTP door — no new
transport, no new server, no new port. It takes no params; an unknown-method
caller still gets the standard `-32601`.

```json
{ "schemaVersion": "0",
  "instance": { "name": "yomi-strix", "url": "http://yomi-strix:8710/", "emittedAt": "2026-08-14T00:05:00Z" },
  "graph": { "schemaVersion": "0", "nodes": [ /* … */ ], "edges": [ /* … */ ] } }
```

`instance.name` resolves the `--node-name` flag on `a2a serve` → the
`AOIDE_A2A_NODE_NAME` env var → the OS hostname → the literal `"aoide"`, the
same precedence-chain discipline the rest of the server's config resolution
follows. `instance.url` is this instance's own advertised URL — the same
string the AgentCard's own `url` field carries. `graph` is EXACTLY what
`aoide graph --json` resolves — the SAME `resolve_graph_document`
function both that command and this method call, so no second graph
vocabulary is invented for the wire. Every stage mutation restages
`state/stage/graph.json` for Quickshell automatically, so there is no
separate emit step to keep in sync.

## The `node:*` node convention (graph fold)

`build_graph` ADDITIVELY folds each registered node in as a root node:
`{ id: "node:<name>", kind: "node", name, url, state, children? }`.

- A **fresh** cache contributes `state: "fresh"` plus `children: { nodes,
  edges }` — the node's OWN already-resolved subtree, nested VERBATIM,
  never flattened into this document's own top-level `nodes`/`edges` (so a
  node's ids can never collide with a local id or another node's).
- A **stale or never-pulled** node still surfaces immediately — visible the
  moment `node add` runs, before any pull ever succeeds — with `state:
  "stale"` and no `children`; never a crash, never a silently-dropped node.
  `error` carries the last pull failure's reason when present.

Local graph commands (`session prune`/`session reap`/`graph link`) keep ignoring `node:*`
ids — none of those commands read `node_store` at all, they operate purely
on `sessions.json`'s `SessionRecord`s, so a `node:*` id is simply never a
session id they could match.

## CLI surface

`aoide node add <name> <url> [--autogate] [--no-verify] [--via ssh://…]
[--token-file <path>] [--bearer-secret <name>]` / `remove <name>` /
`pull [<name>]` / `status` / `hub <name> [--clear]` (the single-hub
designation, a last-resort address-resolution preference) — plus the pairing
ceremony's `aoide pair [<name|url|id>]` with its `reject`/`watch`
subcommands, one verb for both the pending listing and the request itself
([[Pairing-Ceremony]]), the per-node capability gate `allow <name> <cap>
on|off`, LAN discovery `discover [--secs <n>]` with
the `advertise on|off` runtime switch (off by default), `node list
[--json]` (the one-glance mesh roster: every known node marked
`●`/`○`/`◆` with its running sessions beneath, read-only), and
`spawn <name> [--yes] -- <text…>`, a signed spawn against a paired node's
own door — see
[[aoide-cli#The `node` group — aoide-to-aoide federation]] for the per-command
behavior. `status --json` carries the full registry row per node — the
deep per-node detail `node list` never duplicates. `aoide mesh [--json]`
and `aoide mesh pair` sit outside the `node` family proper — the first
reads `config.toml`'s declared `[mesh.<name>]` (above) against this same
registry and reports drift, the second converges it by running the pairing
ceremony over every declared node with no verified record; see
[[Doors-and-Nodes#aoide mesh]].

## Sending across the fold — `send --to node/<query>`

[[Conductor-Channel|`send`]]'s `--to` flag resolves a target name
through the same tiered address grammar `aoide who` uses; a `node/<query>`
form is its remote tier. It resolves `<query>` against a registered node's
CACHED graph (the same `state/node-cache/<name>.json` the fold above reads)
and, on a match, delivers the message over A2A `message/send` instead of
queuing a local pending send. The call is always attempted: the receiving
node gates its own delivery through its own [[A2A-Door]] security model, so
a remote send never sits in the sender's local pending queue. `--to` and
`--id` are mutually exclusive on `send`.

## Security — pairing is the verification path

The [[Pairing-Ceremony]] is the only verification between two instances:
a completed pair commits a `verified: true` node record carrying the
node's ed25519 pubkey and a closed `allows` set (`config.toml`'s
`[pairing] defaultGrant`, `["read"]` by default), and the door's per-request gates read that record.
`node_store::resolve_node` answers "who is this caller" on a ladder of
rungs (`CONTRACTS.md` §6 "Legacy escapes"):

- **Signature** (`NodeRung::Signature`) — a paired node's per-request
  ed25519 signature over a canonical string of method, path, timestamp,
  nonce, and the request body's sha256 hex digest — each field trimmed,
  lowercased, NUL-separated including after the last
  (`aoide_storage::wire_auth::canonical_string`, pinned vectors in
  CONTRACTS.md §6) — carried on four headers (`X-Aoide-Node`,
  `X-Aoide-Timestamp`, `X-Aoide-Nonce`, `X-Aoide-Signature`) that arrive
  together or not at all: a partial set is refused `-32007`, never
  downgraded to the lower rungs. **Identity IS the key; the name is a
  label (#63 P-ID5).** The caller is the record whose stored `pubkey`
  verifies the signature — `a2a.rs::verify_signed_request` tries it
  against every `verified` node's key and takes the one that matches,
  never the record `X-Aoide-Node` names. That header carries the signer's
  claimed SELF name for display/attribution only: a claimed-vs-resolved
  mismatch is audited as attribution drift, and the RESOLVED name wins
  everywhere downstream — the `allows` lookup, the `node:<name>` origin
  stamp, the autogate question — so renaming a node locally never breaks
  its inbound signed requests. No verifying key is one `-32007`
  "signature verification failed" whether the key is unknown, the node
  unverified/keyless, or the signature bad — never an existence oracle
  over the registry. When several verified records share the verifying
  pubkey (one remote instance paired under two names —
  `upsert_paired_node` matches by name), the exact-name match wins the
  tiebreak among equally proven keys, and no exact-name match refuses
  `-32007` "ambiguous signer" rather than guessing among records whose
  grants may differ; a key's effective grant set is therefore the UNION
  across every record sharing it — revoking a capability from a key means
  revoking it on every such record. Replay-guarded by a ±120s timestamp
  window (`-32008`, a taught error naming both timestamps) and a bounded,
  per-`a2a serve`-process nonce cache keyed on `(pubkey, nonce)`, capped
  at 4096 entries (`-32009`); the cache clears on restart, which is why
  both checks run independently. The ONLY rung Spawn accepts:
  `spawn_admitted` requires a verified node, resolved by signature, whose
  `allows` contains `"spawn"`.
- **Token** — a node's own `tokenFile` presented as a bearer. A legacy
  escape for an unpaired caller: it authenticates the read arms
  (`tasks/get`, the AgentCard GET, `aoide/graphSummary`) and answers
  Inject's autogate question, and never reaches Spawn.
- **Addr** (`NodeRung::Addr`) — a bare TCP-source-IP-vs-registered-`url`
  match. Resolves a node identity for ATTRIBUTION only (Inject's `from`
  field, the autogate question): it carries no possession proof and is
  never sufficient for Spawn.
- **Door-wide bearer** (`aoide.a2a.tokenFile`/`aoide.a2a.bearerSecret`) —
  the second legacy escape; it authenticates the read arms without ever
  resolving a node identity.

Inject delivery still classifies the caller's address first
(`a2a::classify_origin` → `ConnOrigin`: `Loopback` / `Remote(IpAddr)` /
`Unknown`). A `Remote` caller falls back to the same interactive
pending-approval queue `send` uses, auto-delivering only on an autogate
match — the OR of the address check, the per-node `tokenFile` check, and
the signature-rung `autogate` flag on the resolved node's own record
(`autogate_match` = `ip_autogate || token_autogate || sig_autogate`). An
`Unknown` origin never auto-delivers. A verified signature outranks
loopback for this question: an ssh `-L` forward delivers a tunneled
node's packets from its own end's sshd, so `origin_for_inject` strips
`ConnOrigin::Loopback`'s free pass from any request
`verify_signed_request` already verified — a signed node's delivery
timing is decided by its `autogate` flag, never by which address it
arrived from ([[Node-Transport]]). `Loopback` keeps its unconditional
trust only while no server-wide inbound bearer gate is configured
([[A2A-Door#Security and governance]]): `aoide.a2a.bearerSecret` (a
secret NAME resolved fresh on every request through the local
[[Secrets-Broker]], consumer `a2a-door`, failing closed on a resolve
failure) takes precedence over `aoide.a2a.tokenFile` (a path read once at
`a2a serve` launch); once either is set, a caller — loopback included —
that does not present the valid token is coerced to `Unknown` before this
classification is even consulted, since behind a reverse proxy or tunnel
every caller's connection looks loopback to the server.

Every inject/spawn/error, loopback or not, writes to the single audit log
as `Door::A2a`, and a resolved node's name is stamped on audit lines,
pending-queue entries, and a spawned session's record as
`origin: "node:<name>"` — the RESOLVED name (attribution drift audited
aside), write-once and door-stamped per [[Session-Graph]]'s identity
section.

## Status

Real: the registry, the cache, `aoide/graphSummary`, the CLI commands, and the
graph fold all run — proved end to end by the same-network integration
test. **Out of scope for this v0** (explicitly, not an oversight):
WAN/NAT-traversal/relay reachability for nodes not on the same network;
Melete-side consumption (a polling skill, first-class graph rendering) —
both are later, separately-directed work.

## Related

- [[A2A-Door]]
- [[Pairing-Ceremony]]
- [[Node-Transport]]
- [[Session-Graph]]
- [[Governance]]
- [[aoide-cli]]
- [[Agent-Interface]]
