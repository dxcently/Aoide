---
type: concept
created: 2026-08-15
updated: 2026-08-25
tags: [aoide, agent, a2a, orchestration, interop, federation]
---

# Peer Federation — aoide-to-aoide graph folding

Peer federation is aoide's fourth door onto itself: one aoide instance
registers ANOTHER aoide instance as a **peer** by URL and pulls its resolved
session graph into its own, folded in as a subtree. It is not a new
transport — it is built entirely on top of the existing [[A2A-Door]] (§6):
one new JSON-RPC method (`aoide/graphSummary`), a client-side peer registry
plus a per-peer pull cache, and an additive fold in the same `build_graph`
function the A2A door's `kind:"a2a"` fold already uses. Spec: `CONTRACTS.md`
§7 (v0, 2026-08-14).

**Melete-optional** — federation works standalone; nothing in it references
or requires [[Melete]]. A Melete-side consumer (a polling skill, first-class
graph rendering) is separate, independently-owned work, not part of this
contract.

**Topology-blind, same-network only in practice.** A peer is addressed by a
plain URL — the protocol itself carries no notion of "same LAN" vs.
"tailnet" vs. "the internet"; any reachable URL works the same way. The
house runs peers over Tailscale (addressed by tailnet MagicDNS hostname),
but nothing here is tailnet-specific. The integration test
(`pkgs/aoide/crates/cli/tests/peer_connectivity.rs`) proves the real
mechanism end to end: two actual `a2a::serve()` instances, a genuine `curl`
AgentCard fetch for `peer add`, a genuine `curl` POST of `aoide/graphSummary`
for `peer pull`, a genuine cache write, a genuine graph fold. Both binds stay
strictly loopback (`127.0.0.1:<port>`), same-subnet HTTP reachability
standing in for "two boxes on the same network." **WAN /
NAT-traversal / relay reachability for a peer that is NOT on the same
network is explicitly out of scope for this v0** — a later, separate
contract amendment, not designed or assumed here.

## The registry — `state/peers.json`

The set of other aoide instances this one has registered, written atomically
via `aoide_storage::peer_store`. Lives in the gitignored root-runtime
`state/` dir — **not** `song/stage/`: a peer roster is account/global
external-registry state, exactly like the sibling `state/a2a-agents.json`
the [[A2A-Door|A2A client]] already keeps there, not song-scoped rehearsal
state. Additive/tolerate-missing: an absent file just means "no peers
registered," never an error.

```json
{
  "schemaVersion": "0",
  "peers": [
    { "name": "yomi-strix", "url": "http://yomi-strix:8710/", "autogate": false, "addedAt": "2026-08-14T00:00:00Z" }
  ]
}
```

`autogate` (default `false`) is the cross-device analogue of `graph send`'s
local "sender is the target's own parent" autogate rule: a peer marked
`true` has its inbound `message/send` auto-deliver without the pending
queue even though the connection is non-loopback — see
[[#Security — the non-loopback pending-gate amendment]] below. An
unmarked/unknown sender is never autogated.

`tokenFile` (`peer add --token-file <path>`, optional) names a file on THIS
instance holding the shared secret that peer presents as `Authorization:
Bearer <token>` — a per-peer credential that identifies WHICH registered
peer is calling once address alone can't (behind a proxy or tunnel every
caller's address looks the same). Absent by default: an unmarked peer
authenticates by address only.

`bearerSecret` (`peer add --bearer-secret <name>`, optional) is the opposite
direction: the name of a secret THIS instance resolves through its own local
[[Secrets-Broker]], as consumer `a2a-client`, and presents as `Authorization:
Bearer <value>` on every OUTBOUND call to that peer's A2A door. It is
resolved fresh on every request, never cached in `peers.json` or anywhere
else, so revoking the underlying secret takes effect on the very next call.
Absent by default: an unmarked peer's outbound requests carry no bearer.

## The cache — `state/peer-cache/<name>.json`

One peer's last-**pulled** `aoide/graphSummary` response, written by `aoide
peer pull [<name>]`. Sibling of `state/peers.json` — same dir family, same
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

`instance`/`graph`/`fetchedAt` are the peer's own response from its LAST
SUCCESSFUL pull, verbatim — `graph` is that peer's own already-resolved
`graph.json` v0 document, unmodified. A **failed** pull (unreachable,
timeout, non-200, malformed body) never deletes this file or clears these
fields: it sets `stale: true` and `lastError` to a short reason, preserving
the last-known-good `instance`/`graph` — one peer being down must never
blank it out of the fold, and pulling several peers must never let one
failure abort the others (each peer's outcome is independent).

Freshness is governed by a single named constant,
`aoide_storage::peer_store::PEER_CACHE_TTL_SECS` (5 minutes) — not a magic
number re-typed at each call site. An entry is `fresh` when `stale == false`
AND `fetchedAt` is within the TTL of now; otherwise `stale` (covers both an
explicit failure mark and a plain TTL expiry). `aoide peer status` and the
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

`instance.name` resolves the `--peer-name` flag on `a2a serve` → the
`AOIDE_A2A_PEER_NAME` env var → the OS hostname → the literal `"aoide"`, the
same precedence-chain discipline the rest of the server's config resolution
follows. `instance.url` is this instance's own advertised URL — the same
string the AgentCard's own `url` field carries. `graph` is EXACTLY what
`aoide graph view --json` / `graph emit` resolve — the SAME
`resolve_graph_document` function both those commands and this method call, so
no second graph vocabulary is invented for the wire.

## The `peer:*` node convention (graph fold)

`build_graph` — the same function that already folds registered A2A agents
in as opaque `kind:"a2a"` root nodes — ADDITIVELY folds each registered peer
in as a root node, one level richer than the A2A fold:
`{ id: "peer:<name>", kind: "peer", name, url, state, children? }`.

- A **fresh** cache contributes `state: "fresh"` plus `children: { nodes,
  edges }` — the peer's OWN already-resolved subtree, nested VERBATIM,
  never flattened into this document's own top-level `nodes`/`edges` (so a
  peer's ids can never collide with a local id or another peer's).
- A **stale or never-pulled** peer still surfaces immediately — visible the
  moment `peer add` runs, before any pull ever succeeds — with `state:
  "stale"` and no `children`; never a crash, never a silently-dropped peer.
  `error` carries the last pull failure's reason when present.

Local graph commands (`graph focus`/`prune`/`reap`/`link`) keep ignoring
`peer:*` ids exactly as they already ignore `a2a:*` ids — none of those
commands read `peer_store` (or `a2a_store`) at all, they operate purely on
`sessions.json`'s `SessionRecord`s, so a `peer:*`/`a2a:*` id is simply never
a session id they could match.

## CLI surface

`aoide peer add <name> <url> [--autogate] [--token-file <path>] [--bearer-secret
<name>]` / `list` / `remove <name>` / `pull [<name>]` / `status` — see
[[aoide-cli#The `peer` group — aoide-to-aoide federation]] for the per-command
behavior. Registered as its own group, directly after `a2a agent
add/list/remove/send` in `schema --json`'s order.

## Sending across the fold — `graph send --to peer/<query>`

[[Conductor-Channel|`graph send`]]'s `--to` flag resolves a target name
through the same tiered address grammar `aoide who` uses; a `peer/<query>`
form is its remote tier. It resolves `<query>` against a registered peer's
CACHED graph (the same `state/peer-cache/<name>.json` the fold above reads)
and, on a match, delivers the message over A2A `message/send` instead of
queuing a local pending send. The call is always attempted: the receiving
peer gates its own delivery through its own [[A2A-Door]] security model, so
a remote send never sits in the sender's local pending queue. `--to` and
`--id` are mutually exclusive on `graph send`.

## Security — the non-loopback pending-gate amendment

The [[A2A-Door]]'s original security model assumes a **loopback** caller and
moves all admission to rebuild time (enabling the door, setting
`spawnAgent`) rather than gating per request. Peer federation is the first
caller that isn't loopback, so `message/send` classifies the caller's
address first (`a2a::classify_origin` → `PeerOrigin`: `Loopback` /
`Remote(IpAddr)` / `Unknown`):

- **`Remote`** — falls back to the same interactive pending-approval queue
  `graph send` already uses, UNLESS the sender's address matches a peer
  registered with `autogate: true` in `state/peers.json`
  ([[#The registry — `state/peers.json`]] above) **or** presents that
  peer's own `tokenFile` secret as a bearer token
  (`peer_store::is_autogated_peer_token`) — either match auto-delivers
  exactly like a loopback call.
- **`Unknown`** (the caller's address couldn't be read at all) — never
  auto-delivered, failing safe the same as an unmatched `Remote`.
- **`Loopback`** — delivers immediately, admission stays at rebuild time,
  **only while no server-wide inbound bearer gate is configured**
  ([[A2A-Door#Security and governance]]). The operator has two ways to set
  that gate, wired through the `aoide-a2a` unit: `aoide.a2a.bearerSecret` (a
  secret NAME resolved fresh on every request through the local
  [[Secrets-Broker]], consumer `a2a-door`, failing closed on a resolve
  failure) takes precedence over `aoide.a2a.tokenFile` (a path read once at
  `a2a serve` launch) when both are set; either alone is sufficient to close
  the gate. Once one is configured, a caller — loopback included — that does
  not present the valid token is coerced to `Unknown` before this
  classification is even consulted: behind a reverse proxy or tunnel every
  caller's connection looks loopback to the server, so an unconditional
  loopback trust hands a remote attacker the operator's own standing.

This is the one interactive per-request gate the wire otherwise lacks —
added specifically for the case the original loopback-only design didn't
need to cover. Every inject/spawn/error, loopback or not, still writes to
the single audit log as `Door::A2a`.

## Status

Real: the registry, the cache, `aoide/graphSummary`, the CLI commands, and the
graph fold all run — proved end to end by the same-network integration
test. **Out of scope for this v0** (explicitly, not an oversight):
WAN/NAT-traversal/relay reachability for peers not on the same network;
Melete-side consumption (a polling skill, first-class graph rendering) —
both are later, separately-directed work.

## Related

- [[A2A-Door]]
- [[Session-Graph]]
- [[Governance]]
- [[aoide-cli]]
- [[Agent-Interface]]
