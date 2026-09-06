# MAIL — store-and-forward letters between nodes, addressed by name, never by transport

The design authority for the mail workstream. Everything in "Settled
decisions" was User-decided through the 2026-09-04 → 09-06 design grill
(three rounds, click-through), informed by the four-system survey in
`docs/Aoide-Wiki/references/P2P-Board-Protocols.md`; executors do not
relitigate it. Where this document and a brief conflict, this document
wins. The default for anything this document leaves unruled: **do it the
way FidoNet netmail did it** (User meta-rule).

## The problem

Aoide's only cross-session channel is conduct: `aoide send` injects bytes
into a live pty. That is a chat box — presence-based, miss-it-lose-it. A
message to a dead session is a clean error and gone; a report from a
remote box to a conductor whose harness id exposes no socket bounces "not
conductable"; the per-host `inbox.json` is a receipt log with two internal
writers that nothing can address or query. Sessions cannot leave each
other letters. Every FidoNet node could.

## Settled decisions

1. **Mail first; boards later.** Mail is addressed, one-to-some,
   store-and-forward. Boards (public, area-flooded, echomail-shaped) are a
   separate later phase on the same store, `aoide board`, cribbed from
   NNCP multicast areas rather than raw FidoNet.
2. **Mail is data at rest, never an instruction.** A letter sits in the
   store until a reader pulls it. It never auto-enters a session's
   conversation. Tasking an agent stays on the conduct path with its
   gate. This split is the security model; nothing below weakens it.
3. **Addressing is `<node>/<label>`.** "Node" is the noun for a mesh
   member ("peer" survives only where it names the other end of an edge —
   pairing, grants). The label is free text riding IN the letter
   (FidoNet's `toUserName`): the envelope routes to the node, the node
   files the letter under the label. **No mailbox registry** — a mailbox
   exists by being named in a `to` or read by name.
4. **Filing, not secrecy.** Every agent on a node is the same unix user;
   any intra-host lock would be fiction. A label says who a letter is
   *for*, never who may read it. FidoNet netmail was sysop-readable; so
   is this.
5. **Deposits are ungated for paired nodes.** Pairing is the trust
   (per-link, FidoNet's pkt password made real). A new `message`
   capability joins the closed `PEER_CAPABILITIES` vocabulary; a verified
   node holding it deposits without per-letter approval. Conduct's gate
   (`pending.json`, autogate) is untouched and mail never queues there.
6. **Deposit-always, then a doorbell.** Writing to the store IS delivery.
   A live session the letter is for gets a fixed nudge; reading is a pull.
7. **Per-reader cursors.** Entries are immutable; each reader keeps its
   own high-water mark (the NNTP `.newsrc` shape). No read bit on the
   entry.
8. **One store.** The receipt log is absorbed: today's `inbox.json` rows
   become typed `receipt` entries in the mailbase; its two writer seams
   repoint; `inbox.json` and `INBOX_CAP` retire.
9. **The full BSO outbox.** A per-node spool at the sender
   (`state/outbox/<node>/`), a transfer-invariant content-hash `msgid` on
   every envelope, flavors `now`/`hold`, per-link backoff/try state.
   Entries live until the destination's end-to-end acknowledgement.
10. **Envelopes are origin-signed.** The sending node signs the letter
    with its pairing key; the destination verifies the ORIGIN regardless
    of path. A transit node can delay, drop, or read — never forge.
11. **Mail routes through hubs, zone-scoped, in v1.** This amends
    PAIRING's kill-list with one carve-out: mail TRANSIT through declared
    hubs. Pairing, trust, and code-ferrying rulings are untouched — a hub
    relays letters, never trust. A hub routes only between nodes declared
    in the same `[mesh.<name>]` it shares with both ends; cross-mesh
    transit only through a node declared as the zonegate between two
    named meshes. The routing table IS the mesh declaration.
12. **Transit files at the hub** (FidoNet-faithful): a typed `transit`
    entry in the hub's mailbase, browsable, re-exported from there.
13. **Wire: extension methods, policy keyed on the method name.**
    `aoide/mailDeposit` and `aoide/mailPoll` beside the pairing methods.
    One door, one inspection, separate windows; gate/no-gate never
    depends on payload shape.
14. **Flood control is Fido-pure: dedup + excommunication.** No quotas,
    no eviction, no auto-expiry. The `msgid` seen-set drops repeats; a
    flooding node is declared `down`. Accepted exposure: between flood
    start and the User's `down` gesture, disk absorbs it. Audit counters
    per origin make the flood visible fast.
15. **Declared node status: `hold` and `down`.** In the mesh config,
    per node. `hold` queues at the sender silently; `down` refuses fast
    and stops routing to/from that node. The UNREACHABLE vocabulary
    ruling (task #50) lands in the same change.
16. **Meshes are zones.** Trust is per edge and flat (`peers.json`); the
    mesh declaration partitions ROUTING, not trust. Two meshes naming the
    same node with different grants is a loud conflict at drift time.

## Vocabulary

| word | meaning | FidoNet ancestor |
|---|---|---|
| node | a mesh member: one Aoide instance, one keypair | node |
| edge / peer | a verified pairing between two nodes; where grants live | link, pkt password |
| mesh | a named `[mesh.<name>]` declaration; a routing zone | zone |
| hub | a node declared in a mesh as a transit relay for that mesh | hub / host |
| zonegate | a node declared as the only transit between two named meshes | zonegate |
| label | free-text recipient name inside a letter | `toUserName` |
| letter | an envelope of type `letter` | netmail message |
| envelope | the signed, immutable unit that moves: header + text | packed message |
| msgid | hash of the signed envelope; global, transfer-invariant | `^AMSGID` (as NNCP MsgHash) |
| mailbase | a node's append-only store of entries | message base |
| outbox | the sender-side per-node spool | BSO flow files |
| flavor | `now` (deliver or queue+retry) / `hold` (wait to be polled) | `?ut` flavors |
| cursor | a reader's high-water mark over the mailbase | `.newsrc` |
| receipt | an envelope of type `receipt`: a conduct-delivery record or an ack | ReturnReceipt |

## Store

`$AOIDE_ROOT/state/mail/`:

```
base.jsonl      append-only, one entry per line, immutable once written
cursors.json    { "<label>": <seq>, ... }        reader high-water marks
seen.json       { "<msgid>": <receivedAt>, ... } dedup memory; outlives pruning
```

An entry is an envelope plus local facts:

```json
{
  "seq": 4127,                       // local arrival order — the cursor axis
  "receivedAt": "2026-09-06T10:41:00Z",
  "type": "letter",                  // letter | receipt | transit
  "envelope": {
    "msgid": "<hex sha256 of sig‖header‖text>",
    "from":  { "node": "osaka", "label": "rebuild-osaka" },
    "to":    { "node": "chiyo", "label": "rebuild-reports" },
    "mesh":  "home",                 // the zone this letter is routed in
    "flavor": "now",
    "mintedAt": "2026-09-06T10:40:58Z",
    "text": "…",
    "sig": "<origin ed25519 over header‖text>",
    "transit": [ { "node": "sakaki", "at": "…", "sig": "…" } ]
  }
}
```

- `seq` is local to the node, like an NNTP article number — never crosses
  a link. Cursors are keyed by label and hold the last `seq` consumed.
- `msgid` = sha256 over the origin signature concatenated with the signed
  bytes. Signature-then-hash makes identical texts distinct letters
  (`mintedAt` is inside the signed header) and makes the id
  transfer-invariant: no hop can alter the envelope without changing it.
- `type=receipt` entries carry what `inbox.json` carried (from, target,
  text, receivedAt, context) inside the same envelope shape with a local
  self-signature — the migration is a field mapping, and acks are
  receipts addressed back to the origin.
- `type=transit` entries are letters the node is relaying (§Transit);
  readers hide them unless asked.
- Reading never mutates an entry. `aoide mail read --for <label>` prints
  entries above the label's cursor and advances it.
- Keep-all. `aoide mail rm --older <duration>` is the only pruning; it
  never touches `seen.json`, so a pruned letter re-offered later is still
  a duplicate (the RFC 5537 §3.3 coupling, kept by construction).

The test-debris guard lands with the store: the mail path derives only
from `AOIDE_ROOT`, and the test-support crate sets a per-test root so no
fixture can ever write to a real `~/.aoide` again (task: the unguarded
`cargo test` that wrote 200 rows into the live inbox).

## Addressing and filing

`aoide mail send --to <node>/<label> [--hold] -- <text>`:

- `<node>` is a petname from `peers.json` or `self`. `<label>` is free
  text, clamped by the existing sanitize rules before any rendering.
- The sender's `aoided` resolves everything else — `via` from the peer
  record, the route from the mesh declaration. **The agent never sees a
  transport line.** A letter to `self/<label>` files locally and rings
  the doorbell; it is how a session leaves a note for a role on its own
  box.
- Arrival files under `to.label`, whether or not any session by that
  name exists or ever will. No bounce for unknown labels: the mailbox
  exists because the letter named it.
- `from.label` is the sending session's petname (sanitized), `from.node`
  is asserted by the origin signature — a node cannot claim another
  node's name, and a remote label is display text only.

## Wire

Both methods enter through the one A2A door and pass the shared checks
first — tunnel, signature verification, origin classification, audit —
then dispatch by method name (`server/a2a.rs`, beside
`aoide/pairRequest`). CONTRACTS §6 gains both at P-M2.

- **`aoide/mailDeposit`** `{ envelope }` → `{ msgid, outcome }`, outcome
  ∈ `accepted | duplicate | refused`. Policy: caller is a verified node
  holding `message`; caller's node is not `down`; origin signature
  verifies; `envelope.mesh` is a mesh the receiver declares. Then: dedup
  by `msgid` (a duplicate is `duplicate`, never an error — hops retry
  freely), file (`letter` if `to.node` is self, else `transit`), ring
  the doorbell, and if `to.node` is not self, re-spool (§Transit).
  Receipts (acks) are envelopes deposited through the same method — one
  routing path for everything, so acks traverse hubs for free.
- **`aoide/mailPoll`** `{ node }` → `{ envelopes[] }`. The caller asks
  "anything waiting for me?" and the receiver hands over every outbox
  entry spooled for the caller's node — all `hold`-flavored ones, and
  `now`-flavored ones too if the receiver's own attempts have been
  failing. The caller must BE `node` (signature), hold `message`, and not
  be `down`. Handed-over entries stay in the outbox until acked like any
  other. This is BSO Hold's missing half: without it a hold flavor could
  never drain across an edge that only one side can start.
- **Acks** are `receipt` envelopes from the destination to the origin,
  minted on filing a `letter`, routed like any letter, never themselves
  acked. At the origin an ack retires the outbox entry with the matching
  `msgid` and files as a receipt.

## Outbox

`$AOIDE_ROOT/state/outbox/<node>/`:

```
<msgid>.json    the envelope, with flavor and route target
link.json       { "tries": n, "lastTry": ts, "holdUntil": ts, "lastError": "…" }
.bsy            lock while a drain session with this node runs
```

- `aoide mail send` writes the entry first, then attempts delivery. The
  write is what the command reports; delivery is best-effort at that
  moment and the spool's job afterwards. A dead peer is files that wait.
- The drain runs on `aoided`'s existing timer cadence and immediately
  after any successful contact with that node (a deposit, a poll, an
  ack). `now` entries are attempted on every drain; `hold` entries are
  never attempted — they leave only through the node's `mailPoll`.
- `link.json` is `.hld`/`.try`: exponential backoff on failure, cleared
  on success. A `down` node's directory is left alone entirely.
- An entry retires only on an ack for its `msgid` (or `aoide mail outbox
  rm`). `aoide mail outbox [<node>]` answers "did it land" truthfully:
  waiting, held, tries, last error, acked.

## Transit

Additive keys in the mesh declaration (deny_unknown_fields — the same
deploy-order rule as the mesh keys themselves: every box switches before
any declaration uses them):

```toml
[mesh.home]
grant = ["read", "spawn", "message"]
hubs  = ["sakaki", "yomi-strix"]        # transit relays for THIS mesh

[mesh.home.peers]
osaka = "ssh://khoa@192.168.1.201"
# …

[mesh.home.status]                       # absent = normal
chiyo = "hold"

[mesh.home.gates]                        # cross-mesh transit, by mesh name
work = "osaka"                           # osaka is the only home⇄work path
```

Routing, at the sender and at every hop, for `to.node`:

1. A direct verified edge to `to.node` → spool to it.
2. Else, for each mesh declaring both self and `to.node`: the mesh's
   hubs that are direct edges of self → spool to the first reachable.
3. Else, `to.node` in a mesh self does not share: the declared gate
   between self's mesh and that mesh → spool to the gate.
4. Else refuse: `no route to <node>` — the sender sees it at send time.

At a hub, a deposit whose `to.node` is not self: verify, dedup, file as
`transit`, append the hub's signature to `envelope.transit`, re-spool by
the same four steps. Loops die twice over: `msgid` seen, and any
envelope whose transit record already names self is dropped. A hub only
relays inside a mesh it declares and only for nodes that mesh declares —
membership in two meshes never makes a node a bridge; being a gate is a
declared role.

Deposit outcome `refused` with reason `no-route`, `down`, `unknown-mesh`,
or `unverified-origin` is returned to the depositing hop, which records
it in `link.json` and stops retrying that entry; the origin learns it
through `aoide mail outbox`.

## Delivery and the doorbell

Filing a `letter` on a node rings the doorbell for every live local
session whose petname equals `to.label` or which holds a cursor for it.
The doorbell is a fixed line injected through the loopback conduct path
(no gate — it is local `aoided` conducting a local session):

```
[aoide mail] new mail for <label-clamped> — aoide mail read --for <label-clamped>
```

No byte of the letter rides the nudge; the label is clamped to the
sanitize alphabet. Sessions without a terminal (headless spawns) still
get it — injection is the same path conduct uses. Nothing else is ever
injected on a letter's behalf.

## Status and the nodelist view

`aoide mesh` is the nodelist command (FTS-5000: "the nodelist defines the
network"). It grows columns: status (`hold`/`down`/normal), role
(hub/spoke/gate), liveness. Statuses are declared in config like every
other mesh fact; the command reports, it does not edit. A later
`aoide mesh down <node>` that edits the declaration for the User is a
convenience allowed by this document, not required by it.

`down` is enforced at the door (deposits and polls from that node are
`refused`), in routing (never a hop, never a destination — `no-route`),
and in the drain (its outbox directory is skipped). `hold` is
sender-side only: entries to that node get flavor `hold` regardless of
the flag, and the drain never attempts them.

## Security model

Threat: code execution as the aoide unix user on one node.

- **On that node: total.** Keys, roster, ssh, stores, logs. Host
  security's jurisdiction. Mail's duty is to not AMPLIFY it.
- **Across edges, as that node's identity:** `read` (recon), `message`
  (deposit letters — inert data), `spawn`/conduct (gated as ever),
  transit (relay letters for its meshes). Origin signatures stop
  impersonation; zone-scoped routing stops a compromise in one mesh
  from reaching another except through a declared gate; `down` is the
  quarantine gesture and keeps the pairing record for forensics.
- **Prompt injection is the real attack** and decision 2 is the wall:
  attacker text lands as a labeled, provenance-stamped entry that a
  reader pulls on purpose. The doorbell carries none of it.
- **Floods** are visible (audit counters per origin), bounded only by
  disk until `down`. Quotas are addable at the door later without any
  schema change; they are deliberately not in v1.
- **Not a multi-operator design** — filing-not-secrecy and files-at-hub
  are same-operator rulings. The primitives (origin identity, msgid,
  zones, gates, acks) are inter-operator grade because they were taken
  from an inter-operator network; end-to-end encryption to the
  destination key (NNCP's shape) and nodelist distribution are the two
  additions such a use would need. This document forecloses neither.

## Commands

```
aoide mail                          new letters for the caller's petname (above cursor)
aoide mail send --to <node>/<label> [--hold] -- <text>
aoide mail read [--for <label>] [--all] [--transit]     print + advance cursor
aoide mail outbox [<node>] [rm <msgid>]                 the spool, truthfully
aoide mail rm --older <duration>                        prune the base, never seen.json
aoide mesh                          nodelist view: + status, role, liveness columns
aoide board …                       reserved; not in this workstream
```

`aoide send`, `aoide pair`, `aoide mesh pair` are unrenamed — none of
them lies under the new model. Golden count changes ride each phase's
commit with the full count-site checklist.

## Phases

Serialized, exec + different reviewer each, cargo field exclusive per
phase, docs in the same commit (this file, CONTRACTS, the crate
READMEs), no subagent spawning and no backgrounded cargo in any brief.

- **P-M1 — the mailbase, locally (M).** `storage::mail`: base/cursors/
  seen, entry + envelope types, local self-signed envelopes, `msgid`.
  Commands `mail`, `mail send` (to `self/<label>` only), `mail read`,
  `mail rm`. The two `inbox::receive` seams repoint to
  `mail::file_receipt`; `inbox.json` migrates on first open; `inbox.rs`
  and `INBOX_CAP` deleted. Test-root guard in test-support. Tests:
  append-only invariants, cursor advance, migration mapping, seen-set
  survives `rm`, no fixture escapes the test root.
- **P-M2 — envelopes on the wire, direct edges (L).** Origin signing
  and verification (the pairing keypair), `message` capability,
  `aoide/mailDeposit` in the door (CONTRACTS §6), the outbox spool with
  `now` flavor + `link.json` backoff, the drain on the timer, ack
  receipts. `mail send --to <node>/…` for direct edges; `mail outbox`.
  Tests: sign/verify round-trip, duplicate is not an error, dead-peer
  entry waits and drains on return, ack retires exactly its `msgid`.
- **P-M3 — hold and poll (M).** `aoide/mailPoll`, `--hold`, the drain's
  never-attempt rule for hold, poll-on-contact. Tests: a hold entry
  drains only via poll; a poller receives only its own entries; a
  non-`message` poller is refused.
- **P-M4 — zones (L).** `hubs`/`gates`/`status` keys (validate_mesh,
  drift shows them), the four-step router, transit filing + signature
  chain + loop guards, `down`/`hold` enforcement at door/route/drain,
  `aoide mesh` columns, #50 vocabulary. PAIRING's kill-list carve-out
  is already worded; the code gate that refuses relay must except
  `mailDeposit` transit explicitly. Tests: the five-edge fixture mesh —
  osaka→chiyo routes via a hub, an unshared mesh gets `no-route`, a gate
  bridges two meshes, a looped envelope is dropped once, `down` refuses
  at all three points.
- **P-M5 — doorbell + polish (S).** The fixed-line nudge through
  loopback conduct, headless-session coverage, `mesh down` convenience
  if wanted, outbox report wording. Tests: nudge text contains no
  letter bytes, label clamp holds for adversarial labels.

Verification gate per phase: the crate's own tests green, `aoide schema
--json` golden updated in the same commit, and a live two-node run
(yomi ⇄ osaka) of the phase's headline command recorded in the session
ledger. P-M4's live gate is the routed letter osaka → sakaki → chiyo,
which needs chiyo lit and the mesh declarations deployed in the ruled
order.

## Kill-list

- No mailbox registry, no `mailbox create` — a box exists by being
  named (User ruling).
- No quotas, no eviction, no auto-expiry — dedup + `down` are the whole
  flood story in v1 (User ruling; quotas remain addable at the door).
- No public flag on mail — public is a board post, a later phase.
- No end-to-end encryption — same-operator filing-not-secrecy; noted as
  the first addition a multi-operator use would need.
- No remote read: readers read their own node's base; letters travel,
  readers never do.
- No letter content in any doorbell, notification, or log line — fixed
  text plus a clamped label, ever.
- No transit outside a shared mesh except through a declared gate; no
  implicit bridging by dual membership.
- No relay of trust, codes, or pairing state — the PAIRING kill-list
  stands; mail transit is the one named carve-out.
- No per-mesh identity — one keypair per node.
- No cursor sync across nodes — cursors are reader-local, like every
  surveyed system.
- No mail through `message/send` — instruction and data never share a
  wire method.
