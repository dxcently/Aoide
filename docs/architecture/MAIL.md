# MAIL — store-and-forward letters between nodes, addressed by name, never by transport

The design authority for the mail workstream. Everything in "Settled
decisions" was User-decided — the 2026-09-04 → 09-06 design grill (three
rounds, click-through), informed by the four-system survey in
`docs/Aoide-Wiki/references/P2P-Board-Protocols.md`, and the 2026-09-25/26
rulings on sealing, rosters, trust per mesh and boards; executors do not
relitigate it. [HTTPS-MESH-API.md](HTTPS-MESH-API.md) owns the sealed
container, the key bindings, the roster's trust rules and the threat model;
this document owns the store, the addressing, the routing, boards and the
slice order. Where this document and a brief conflict, this document wins.
The default for anything this document leaves unruled: **do it the way
FidoNet netmail did it** (User meta-rule).

## Letter presentation and copies

`mail send --to node/name --subject "Subject" --cc other/name -- text`
places Subject, To, Cc and body in `AOIDE-LETTER/1` JSON inside the signed
text. Optional `threadId` identifies the letter's thread — the reply chain of
one send and its answers — and `replyTo` identifies the parent envelope. Both
are 64 lowercase hexadecimal characters. New structured sends mint one secure
random thread ID shared by every copy. Replies pass `--thread` and
`--reply-to`; a legacy reply uses its original message ID as the thread
anchor. Forwarding omits both and starts a new thread. These fields are signed
context, never proof of membership or delivery. A thread has no members and no
key; a group conversation with members is a board (§Boards).
With any structured-content flag, To and Cc accept comma-separated
addresses. The original envelope header remains the actual destination;
Cc metadata names intended recipients, not confirmed delivery. Legacy
letters remain plain text and invalid structured content displays verbatim.

Every unique recipient receives an independently signed envelope through
the scalar send path. All addresses and paired nodes validate before any
copy is filed. `self` and the canonical local host resolve to local filing.
The result reports each recipient's msgid, filing/spooling state and delivery
projection. Partial filing returns an error with accepted copies retained;
callers retry only failed copies. Fanout is not a crash-atomic transaction.
Existing outbox retries retain each envelope's msgid; invoking send again
creates new letters. No review/hold or interception policy is implied.

## The problem

Aoide's only cross-session channel is conduct: `aoide send` injects bytes
into a live pty. That is a chat box — presence-based, miss-it-lose-it. A
message to a dead session is a clean error and gone; a report from a
remote box to a conductor whose harness id exposes no socket bounces "not
conductable"; the per-host `inbox.json` is a receipt log with two internal
writers — `inbox list/read/clear` can show it, but nothing can DEPOSIT
into it by address. Sessions cannot leave each other letters. Every
FidoNet node could.

## Settled decisions

1. **Letters first; boards on top of them.** Mail is addressed,
   one-to-some, store-and-forward: a letter goes to a mailbox. A board is
   every group conversation — a two-member direct board, a project's
   board, an organization's or a club's — with members and a key per
   epoch (§Boards). Boards are a later phase on the same store and the
   same transport, `aoide board`: every post travels as a letter, and a
   letter to a mailbox stays a letter.
2. **Mail is data at rest, never an instruction.** A letter sits in the
   store until a reader pulls it. It never auto-enters a session's
   conversation. Tasking an agent stays on the conduct path with its
   gate. This split is the security model; nothing below weakens it.
3. **Addressing is `<node>/<name>`.** "Node" is the noun for a mesh
   member — the one noun for it in code, config, wire, and docs; "edge"
   names a verified pairing between two nodes. The name is free text
   (FidoNet's `toUserName`): the envelope routes to the node, the node
   files the letter under the name. **No mailbox registry** — a mailbox
   exists by being named in a `to` or read by name.
4. **Filing, not secrecy — on the node.** Every agent on a node is the
   same unix user; any intra-host lock would be fiction. A name says who a
   letter is *for*, never who may read it. FidoNet netmail was
   sysop-readable; so is a node's own mailbase. Between nodes it is the
   opposite: decision 17 seals every letter that leaves its node.
5. **Deposits are ungated per letter, not per node.** Trust in a mesh
   (decision 16) is what admits a node — its pairing, or its line on the
   mesh's roster — and a `message` capability in the closed
   `NODE_CAPABILITIES` vocabulary is what lets it deposit: a node holding
   `message` in the letter's mesh deposits every letter without further
   approval, and one that does not hold it there deposits nothing.
   `message` is an explicit widening like `spawn`, never part of
   `[pairing] defaultGrant` — opening a mail link is a gesture the User
   makes, which is also the moment the bearer-token fix below is due. A
   roster line's default grant is `message`, because writing the line
   is that gesture. Conduct's gate (`pending.json`, autogate) is untouched
   and mail never queues there.
6. **Deposit-always, then a doorbell.** Writing to the store IS delivery.
   A live session the letter is for gets a fixed nudge; reading is a pull.
7. **Per-reader cursors.** Entries are immutable; each reader keeps its
   own high-water mark (the NNTP `.newsrc` shape). No read bit on the
   entry.
8. **One store.** The receipt log is absorbed: today's `inbox.json` rows
   become typed `receipt` entries in the mailbase; its two writer seams
   repoint; `inbox.json`, `INBOX_CAP`, and the `inbox` commands retire.
9. **The full BSO outbox.** A per-node spool at the sender
   (`state/outbox/<node>/`), a transfer-invariant content-hash `msgid` on
   every envelope, flavors `now`/`hold`, per-link backoff/try state.
   Entries live until the destination's end-to-end acknowledgement.
10. **Envelopes are origin-signed.** The sending node signs the letter
    with its identity key; the destination verifies the ORIGIN regardless
    of path. A transit node can delay or drop — never read (decision 17)
    and never forge.
11. **Mail routes through hubs, zone-scoped, in v1.** This amends
    PAIRING's kill-list with one carve-out: mail TRANSIT through declared
    hubs. Pairing and code-ferrying rulings are untouched — a hub relays
    letters, never trust; a roster riding through a hub carries the
    operator's signature, not the hub's word. A hub routes only between
    nodes of a mesh it shares with both ends; cross-mesh transit only
    through a node declared as the zonegate between two named meshes. The
    routing table IS the mesh declaration — the roster, for a roster
    mesh.
12. **Transit files at the hub** (FidoNet-faithful): a typed `transit`
    entry in the hub's mailbase, holding the sealed container and its
    routing metadata only, browsable as metadata, re-exported from there.
13. **Wire: extension methods, policy keyed on the method name.**
    `aoide/mailDeposit` and `aoide/mailPoll` beside the pairing methods.
    One door, one inspection, separate windows; gate/no-gate never
    depends on payload shape.
14. **Flood control is Fido-pure: dedup + excommunication.** No quotas,
    no eviction, no auto-expiry. The `msgid` seen-set drops repeats; a
    flooding node is declared `down`. Accepted exposure: between flood
    start and the User's `down` gesture, disk absorbs it. The audit log's
    one line per deposit, with origin, is the detection.
15. **Declared node status: `hold` and `down`.** In the mesh config,
    per node. `hold` queues at the sender silently; `down` refuses fast
    and stops routing to/from that node. The UNREACHABLE-vocabulary
    ruling lands in the same change.
16. **Meshes are zones and trust scopes.** A mesh partitions routing AND
    trust: a grant is given in one mesh and holds only there, so a node
    trusted in one mesh gains nothing in another. A mesh is either a
    roster mesh (one operator key signs its nodes and grants) or a pair
    mesh (its trust is the pairwise records made in it, for machines of
    different owners). A node may sit in several meshes with a different
    grant in each; declaring it with two different keys is a load-time
    validation error — one node, one key. Grants are global per node
    until P-ROSTER, which moves every existing pairing into the User's
    home mesh with its grant unchanged. The full rules are
    HTTPS-MESH-API.md's "Trust per mesh".
17. **Every letter that leaves its node is sealed at mint** (from
    P-SEAL): age-encrypted to the destination node's age key, or to a
    board's epoch recipient for a post, inside HTTPS-MESH-API.md's
    container, with the envelope below as its sealed inner plaintext. No
    relay, hub or TLS front holds a key that opens one. Each host
    generates its own keys; private keys never leave it.
18. **One operator's machines share a roster.** The operator keeps one
    signed roster per mesh — node names, identity keys, signed age
    bindings, grants, addresses, relays — and every machine trusts the
    operator key once. For those machines the roster IS the mesh
    declaration and replaces per-pair ceremonies; pairing stays for
    links between different owners. Same operator means same roster
    signer.

## Vocabulary

| word     | meaning                                                            | FidoNet ancestor            |
| -------- | ------------------------------------------------------------------ | --------------------------- |
| node     | one Aoide instance in a mesh: one identity key, one age key        | node                        |
| edge     | a verified pairing between two nodes, made in one pair mesh        | link, pkt password          |
| mesh     | a named routing zone and trust scope: a roster mesh or a pair mesh | zone                        |
| roster   | a mesh's node list, grants and relays, signed by its operator key  | nodelist                    |
| operator key | the Ed25519 key that signs one mesh's roster and board takeovers; the mesh root | zone coordinator            |
| home mesh | the User's own mesh (`[pairing] homeMesh`, default `home`)        | —                           |
| hub      | a node a mesh declares as a transit relay (key `relays`)           | hub / host                  |
| zonegate | a node both of two meshes declare as their gate to each other      | zonegate                    |
| name     | free-text recipient name inside a letter                           | `toUserName`                |
| mailbox  | a `<node>/<name>` address: where letters file; how a board member is named | netmail address     |
| letter   | an envelope of type `letter`                                       | netmail message             |
| envelope | the signed, immutable unit that moves: header + text               | packed message              |
| sealed   | age-encrypted inside the container; opened only at the destination | —                           |
| binding  | a node's age key, signed by its own identity key                   | —                           |
| msgid    | hash of the signed envelope; global, transfer-invariant            | `^AMSGID` (as NNCP MsgHash) |
| mailbase | a node's append-only store of entries                              | message base                |
| outbox   | the sender-side per-node spool                                     | BSO flow files              |
| flavor   | `now` (deliver or queue+retry) / `hold` (wait to be polled)        | `?ut` flavors               |
| cursor   | a reader's high-water mark over the mailbase                       | `.newsrc`                   |
| receipt  | an envelope of type `receipt`: a conduct-delivery record or an ack | ReturnReceipt               |
| thread   | a letter's reply chain (`threadId`); no members, no key            | reply chain                 |
| board    | a group conversation: members, an owner, a key per epoch           | echomail area               |
| post     | a letter on a board, sealed to the board's current epoch           | echomail message            |
| takeover | an operator-signed record moving a board to a new owner node       | —                           |
| epoch    | one period of a board's membership, with its own key               | —                           |
| wrap     | a letter carrying an epoch key to one member node                  | —                           |

`hub` in this document is the transit role. The older per-node
`Node.hub` flag in `nodes.json` (the address-resolution last resort from
PAIRING's discovery phase) is unrelated and untouched — which is why the
config key is `relays`, not `hubs`.

## The envelope

The unit that moves. Three parts with strict byte rules, because the
signature and the id depend on them:

```
header  = canonical string, NUL-joined fields in this fixed order:
          version · from.node · from.name · to.node · to.name ·
          type · mintedAt · originMesh
text    = the letter body, raw bytes
sig     = origin's ed25519 over  header ‖ 0x00 ‖ text
msgid   = hex sha256 over        sig ‖ header ‖ 0x00 ‖ text
```

- The canonical string is **verbatim bytes**: fields joined by NUL, no
  trimming, no case folding, no JSON — so key order and whitespace can
  never make two hops disagree, and no hop can change the case of a
  name without breaking the signature. (The session seal's rule, not
  `wire_auth::canonical_string`, which folds.)
- Each transit hop appends a **chained** hop signature with its own key,
  over length-prefixed `msgid ‖ prev ‖ node ‖ next ‖ at ‖ mesh`: `prev`
  is the hash of the entry before it (`msgid` for the first), `next` is
  the node this hop hands the letter to, and the `mesh` it is carrying the
  letter in is inside, which is what makes a gate's rewrite verifiable.
  The destination walks the whole chain and requires each entry's `next`
  to be the following entry's `node`, and the last `next` to be itself,
  so no hop can cut the chain and re-append itself
  (HTTPS-MESH-API.md, "Hop-signature chain").
- **Outside the signature and the hash:** `flavor` (a sender-side spool
  fact, never on the wire), `mesh` (the routing zone this hop is
  carrying the letter in — rewritten only by a gate), and `transit` (the
  hop chain, appended per hop). Anything a hop may legitimately change
  is outside; anything the destination must trust is inside.
- Signature-then-hash with `mintedAt` signed gives identical texts
  distinct ids, and no hop can alter the signed part without changing
  the id — so a dedup hit is always the same letter.
- `originMesh` is the zone the origin posted in; `mesh` starts equal to
  it and changes only when a gate carries the letter into the other
  mesh it gates.
- Node names on the wire are the **mesh-declared names**, never a box's
  local nickname for a node. Drift reports a nickname that differs from
  the declared name for the same key.

Wire form is JSON around those bytes: `{ header: {…fields}, text, sig,
msgid, mesh, transit: [ {node, next, at, sig} ] }` — the receiver
re-derives the canonical header from the fields and rejects if `msgid`
does not recompute. From P-SEAL on, this envelope is the sealed inner
plaintext of HTTPS-MESH-API.md's container: `header`, `text`, `sig` and
`msgid` travel inside `ct`, byte-identical to the rules above, while
`mesh` and `transit` ride outside it on the container.

## Store

`$AOIDE_STATE_DIR/mail/` (the ordinary `state_dir()` resolution —
`AOIDE_STATE_DIR` first, then `AOIDE_ROOT/state` — like every other state
file):

```
base.jsonl      append-only, one entry per line, immutable once written
cursors.json    { "<name>": { "<reader>": { "seq": n, "rung": n } } }
seen.jsonl      one `{"msgid","receivedAt"}` per line, append-only; dedup
                memory that outlives pruning, loaded once per process
```

An entry is an envelope plus local facts:

```json
{ "seq": 4127, "receivedAt": "…", "type": "letter",
  "via": "sakaki",                   // the hop that deposited it; "self" for local
  "envelope": { … as above … } }
```

- `seq` is local to the node, like an NNTP article number — never crosses
  a link. It is `last line's seq + 1`, read under the lock.
- Three processes write this store: the `aoided` timer, the `aoide a2a
  serve` door (a separate unit, `modules/nucleus/aoided.nix`), and the
  CLI (`mail send`, `mail read`). **Every mutation of the mailbase or an
  outbox runs under the stage lock, and refuses when the lock is not
  held.** Today's `with_stage_lock` (`storage/src/fs.rs`) runs its
  closure even when the flock could not be taken; mail uses a
  `try_stage_lock` that blocks for the same `LOCK_EX` and returns `Err`
  when it cannot take it — a writer waits behind another, never runs
  unlocked. P-M1 adds it beside the existing helper, which keeps its
  fail-open shape for the callers that chose it. Three sessions on one
  box cannot make two append at once.
- Write order is what makes a crash safe: under the lock, truncate a torn
  tail of `base.jsonl` to the last complete line, append the entry,
  fsync, THEN append to `seen.jsonl`. A `seen` line whose `msgid` is
  absent from the base is a crash between those two writes, and that
  `msgid` is accepted again. The reverse order would lose mail. Only
  the write path truncates; a reader skips a torn tail and writes
  nothing, like every other JSONL reader here.
- All three files resolve through the same `state_dir()`; the timer, the
  door unit, and the CLI must see one directory. P-M1's live gate checks
  the unit environments agree, not just the code.
- `type=receipt` entries carry what `inbox.json` carried (from, target,
  text, receivedAt) inside the envelope shape, signed by the box
  identity (`identity::load_or_mint` — a box that has never paired
  mints its key on its first letter). The migration is a field mapping,
  run under the stage lock on the first store open by any command;
  `inbox.json` is renamed to `inbox.json.migrated` in the same critical
  section, so a second process opening concurrently finds either the
  old file or the finished base, never a half-copied one. The old
  per-entry read flag has no high-water counterpart and is not carried:
  every migrated entry is unread once. Acks are receipts addressed back
  to the origin.
- `type=transit` entries are letters the node is relaying (§Transit),
  held sealed — the container and its routing metadata, never an opened
  envelope; readers hide them unless asked.
- `type=post` entries are opened board posts, filed once per node and
  carrying their board id (§Boards). A roster letter and a wrap are
  applied, not filed as correspondence: the roster to `state/mesh/`, the
  epoch key to `state/boards/`; only their `msgid` enters `seen.jsonl`.
- Reading never mutates an entry. Cursors are per reader: a name maps to
  a set of readers, each with its own high-water mark, and a read
  advances only the caller's. Two agents sharing a mailbox never
  consume each other's mail. A reader is identified by its conducting
  session id (`AOIDE_SESSION_ID`), and by the mailbox name itself when
  that is unset — so a stray read from an unconducted terminal advances
  a pseudo-reader and never a live agent's mark. Every process under one
  wrap — sequential harness conversations in the same terminal, a native
  subagent inheriting the variable — shares that one cursor. The reader key is always an executor — a
  session — and never the enduring agent identity (`enduringAgentId`)
  that several executors may share: that identity can name a mailbox
  and resolve a ring target, never a cursor. Two
  unconducted terminals share that pseudo-reader and so still consume
  each other, which is the honest floor: nothing distinguishes them. A
  respawned session is a new reader and sees the name from the
  beginning, the way an NNTP client with no `.newsrc` does: re-delivery
  is the safe direction, and silent loss is not. The reader set is also
  what the doorbell rings.
- Each mark also carries `rung`: the doorbell latch, the highest arming
  `seq` this reader has already been rung for. A reader arms again only
  once its own read catches up to `rung`; omitted from `cursors.json`
  while zero, so a never-rung mark reads byte-identical to before the
  field existed.
- A `cursors.json` carrying the flat `{ "<name>": { "seq", "readers" } }`
  shape migrates on first open under the same lock: each recorded reader
  inherits the name's old `seq`, and a name with no recorded reader
  keeps its mark under the name itself. Nobody re-reads what they had
  already read.
- Keep-all. `aoide mail rm --older-than <Nd|Nh>` is the only pruning; it
  never touches `seen.jsonl`, so a pruned letter re-offered later is
  still a duplicate (the RFC 5537 §3.3 coupling, kept by construction).

The test-root guard lands with the store: `test-support` gains a helper
that points `AOIDE_ROOT` (and so `state_dir()`) at a per-test temp root
under its existing `env_lock`, and every storage test that touches mail
uses it — closing the path by which a fixture once wrote fixture rows
into the live inbox.

## Addressing and filing

`aoide mail send --to <node>/<name> [--hold] -- <text>`:

- `<node>` is a mesh-declared node name or `self`. `self` resolves to
  this box's own node name (`display::local_host_name()`) when the
  envelope is minted, so the literal never enters a header — a letter
  filed locally is the same bytes it would be on the wire. `<name>`
  satisfies the node-name grammar (`^[a-z0-9][a-z0-9-]*$`,
  `valid_node_name`) or the filing is refused before anything is
  written — never clamped, never rewritten.
- **Address role names, never session petnames.** Petnames are minted
  `adjective-noun` per session and change on every respawn; a role name
  (`rebuild-reports`, `conductor`) outlives the session that reads it.
  A brief says "report to `yomi-strix/conductor`", not to a petname.
- **A managed task's mailbox name is its task slug.** `aoide spawn --task
  <slug>` files the run's exit report to `self/<slug>`; the slug is validated
  with the same name predicate every other mailbox name is, so it is always a
  legal address and nothing new is addressed. A slug outlives a respawn, so one
  mailbox can hold several runs' letters — the RUN is the sender's session id,
  and a letter whose sender is not the run being watched is shown as an earlier
  (or unrelated) run, never merged into that run's report. This adds no
  addressing form, no store, and no delivery rule.
- **A task run's two mailboxes stay apart.** `self/<slug>` is the CHILD's own
  inbox: only the letters sent TO the subagent, read against the child's own
  cursor. The wrapper's end-of-run report goes to a REPORT mailbox instead —
  `--report-to <name>`, else the run's parent session id when that is a legal
  mailbox name, else the documented role mailbox `conductor` (the role name the
  addressing rule above names for the conductor). A canonical id that is not a
  legal mailbox name is never rewritten into one, because two identities must
  not collide on a name; a run with no parent and no `--report-to` is reported
  without a letter. `mail::unread_for(name, reader)` is the corresponding
  read-only PEEK — a reader-selected view that advances no cursor, which is what
  lets an observer watch a mailbox without consuming anyone's mail.
- The sender's node resolves everything else — `via` from the node
  record, the route from the mesh declaration. **The agent never sees a
  transport line.** `self/<name>` files locally and rings the doorbell;
  it is how a session leaves a note for a role on its own box.
- Arrival files under `to.name`, whether or not any session by that
  name exists or ever will. No bounce for unknown names: the mailbox
  exists because the letter named it. Bare `aoide mail` therefore always
  prints the names that have unread mail — a mis-typed name is visible
  there, not lost.
- `from.node` is asserted by the origin signature. `from.name` is the
  sending session's petname taken from `AOIDE_SESSION_ID`/`--from`,
  **attribution only** — any same-uid process can claim any name, as
  conduct's own `--from` already documents.

## Wire

Both methods enter through the one A2A door and pass the shared checks
first — tunnel, per-connection signature verification, origin
classification, audit — then dispatch by method name (`server/a2a.rs`,
beside `aoide/pairRequest`). CONTRACTS §6 gains both at P-M2, and the
door's audit name whitelist gains both names so they never log as bare
`a2a.rpc`.

- **`aoide/mailDeposit`** `{ envelope }` → `{ msgid, outcome }`, outcome
  ∈ `accepted | duplicate | refused(reason)`. Admission and outcome are
  different answers and travel differently: whether the caller may speak
  to the method at all is a JSON-RPC **error**, the way every other door
  arm already refuses, and what became of a well-formed envelope is a
  **result**. An admission failure returned as a 200 result would audit
  as `ok` through the door's own error heuristic, and an audit that
  files a refusal as a success is worse than a blunt error code. Step 1
  below is therefore an error; steps 2 onward are outcomes. Policy, in
  order:
  1. caller is a verified node holding `message` in the envelope's
     `mesh`, not `down`. "Verified" means a connection signature that
     checked against ONE key; the hop's identity for every later step is
     the node that key belongs to — never a name the caller wrote in the
     envelope, never a `nodes.json` nickname looked up by name. From
     P-ROSTER the signed request names its mesh, which must equal the
     envelope's `mesh`, and the grant is read in that mesh only: the
     caller's roster line, or its paired record's grant there;
  2. `msgid` recomputes from the envelope;
  3. **zone check:** the receiver declares `envelope.mesh`, and the hop
     from step 1 is declared in it. If `envelope.mesh ≠ header.originMesh`
     the letter has crossed a gate, and the last `transit` element must be
     a hop signature (§The envelope) over this `mesh` from the declared
     gate between the two meshes — else `zone-violation`. A dual-member
     node cannot re-label a letter into its other mesh: its hop signature
     would name a node that is no gate;
  4. origin signature verifies against **the key on record for
     `header.from.node`** in `header.originMesh` — its roster line in a
     roster mesh, its paired record in a pair mesh (§Transit). One key is
     tried, the one that name maps to. Today's connection-signature resolution tries every
     verified key and reports whichever matched; that shape must not be
     reused here, because a paired node signing as another paired node
     would then file under the wrong name. No key on record for that
     node → `unverified-origin`;
  5. dedup: a seen `msgid` returns `duplicate`, never an error — hops
     retry freely. When the duplicate is a filed `letter`, the
     destination re-spools its receipt: a lost ack is the common reason
     for a re-offer, and answering with silence would leave the origin
     retrying against a letter that already landed;
  6. file (`letter` if `to.node` is self, else `transit`), ring the
     doorbell, and if `to.node` is not self, re-spool (§Transit).
  Receipts (acks) are envelopes deposited through the same method — one
  routing path for everything, so acks traverse hubs for free.
- **`aoide/mailPoll`** `{ node }` → `{ envelopes[] }`. The caller asks
  "anything waiting for me?" and receives every outbox entry spooled
  toward the caller's node — all `hold` ones, and `now` ones whose own
  attempts have been failing. The caller's verified identity must BE
  `node` (no polling on another's behalf), hold `message`, and not be
  `down`. Handed-over entries stay in the outbox until acked like any
  other; a re-poll before the ack re-hands them and the receiver's dedup
  makes that harmless.
- **Acks** are `receipt` envelopes minted by the destination on filing a
  `letter`: `to` = the origin, `text` = the acked `msgid`, signed by the
  destination. Routed like any letter, never themselves acked. **An
  outbox entry retires when its delivery is confirmed, and what counts
  as confirmation depends on what the entry is.** For a `letter`: a
  receipt arrives whose verified signer is the entry's `to.node` and
  whose text names the entry's `msgid`. For a `receipt`: the deposit
  outcome itself, `accepted` or `duplicate` — an ack needs no ack
  because an ack IS what confirmation looks like, and a lost one is
  recovered by the origin's next re-offer, which re-spools a fresh
  receipt. Acks are still spooled like any entry, so an ack survives a
  dead link instead of vanishing on it. A hub can mint a receipt; it
  cannot sign as the destination, so it cannot make an origin stop
  retrying.

**Lane and payload.** P-M2 built this wire with an origin-signed envelope
whose `text` is plaintext, carried by the loopback/SSH transport. From P-SEAL
on, both methods carry [HTTPS-MESH-API.md](HTTPS-MESH-API.md)'s sealed
container instead, with the envelope above as its inner plaintext, on every
transport. The policy above then splits in two: every receiver runs the
keyless checks — admission, the outer origin signature, the zone check, dedup
— and only the destination opens `ct` and runs the `msgid` recomputation and
the inner origin signature against the opened envelope (that document's "Two
verification halves"). A hop never opens a letter. The one plaintext path left
is the direct SSH lane to a destination that has published no binding yet;
nothing plaintext ever enters transit.

## Outbox

`$AOIDE_STATE_DIR/outbox/<node>/`:

```
<msgid>.json    the envelope + local facts: flavor, route target, tries,
                lastTry, lastOutcome
link.json       { "holdUntil": ts, "lastError": "…" }   per-link backoff
.bsy            flock'd while a drain session with this node runs
```

- `aoide mail send` writes the entry first, then attempts delivery. The
  write is what the command reports; delivery is best-effort at that
  moment and the spool's job afterwards. A dead node is files that wait.
- The drain runs on `aoided`'s existing timer tick (the reaper cadence)
  and immediately after any successful contact with that node. Because
  the door is a separate process, "after contact" means the door
  process drains under the same locks — the `.bsy` flock is what keeps
  the two drains from racing one link. It is taken non-blocking, the way
  BSO means it: a drain that finds a link busy skips that link and moves
  to the next. Blocking would queue tick after tick behind one stalled
  ssh dial and starve every other link.
- Entries hold the envelope as minted — from P-SEAL, the sealed container
  — so a retry resends the same bytes on any transport.
- A drain session reaches the node by its address's transport — an ssh
  tunnel for `ssh://`, an outbound HTTPS request for `https://` (H1); a
  `poll` node is never dialed — deposits, and **tears any tunnel down
  before it exits** — never leaves a forward standing for the next tick. A standing `ssh -L` is a loopback path into the far
  door for any same-uid process, and the door classifies an unsigned
  loopback caller as local (auto-delivered conduct) unless a bearer
  token is configured. That is a conduct hole older than mail; mail
  does not widen it, and the ops fix — `aoide.a2a.bearerSecret` set on
  every node, so an unsigned caller is Unknown and gated — is
  recommended in the same breath as the first `message` grant.
- `now` entries are attempted on every drain; `hold` entries are never
  attempted — they leave only through the node's `mailPoll`. A node
  declared `hold` in the mesh config makes every entry toward it `hold`
  regardless of the flag.
- `link.json` is `.hld`: exponential backoff per link on failure,
  cleared on success. Per-entry `tries`/`lastOutcome` live in the entry.
  A `refused` outcome parks that entry (drains skip it) and records the
  reason. Parked is not condemned: the refusing grant is the RECEIVING
  node's record of the sender, so once that host runs `aoide node allow
  <sender> message on` (or the operator adds `message` to the sender's
  roster line), the sender runs `aoide mail outbox retry <msgid>` (or
  `--refused [<node>]`) to un-park and dial at once.
  The stored signed envelope is resent as-is, never re-minted, so the far
  end's msgid dedup still holds.
- A `down` node's directory is skipped entirely; entries whose ORIGIN is
  `down` are dropped at the next drain.
- An entry retires only on a valid ack (above) or `aoide mail outbox rm
  <msgid>`; `mail outbox retry` never retires anything. `aoide mail outbox [<node>]` answers "did it land" truthfully
  per entry via `data.delivery` — the same projection `aoide mail send`
  reports right after its own best-effort drain attempt, so neither
  command can drift from the other on what "queued"/"retrying"/etc. mean
  (vocabulary and precedence: "Status and the nodelist view" below).

## Transit

The routing table is the mesh declaration, in one of two places. A
roster mesh's is its roster (HTTPS-MESH-API.md, "Rosters"): `relays`,
each node's `address`, and optional `[status]` and `[gates]`, signed by
the operator; config names only the operator key. A pair mesh's is its
`[mesh.<name>]` section, with additive keys (deny_unknown_fields — the
same deploy-order rule as the mesh keys themselves: **every box switches
before any declaration uses them**; the drift report is the dry run):

```toml
[mesh.friends]                           # a pair mesh: machines of different owners
relays = ["sakaki"]                      # transit hubs for THIS mesh

[mesh.friends.nodes]
evo = "ssh://evo@192.168.1.40"
# …

[mesh.friends.status]                    # absent = normal
evo = "hold"

[mesh.friends.gates]                     # cross-mesh transit, by mesh name;
home = "sakaki"                          # the OTHER mesh must declare it back
```

- **A node may be declared in several meshes** (P-M4 relaxes today's
  cross-mesh uniqueness check in `validate_mesh`), with a different grant
  in each (decision 16); two different keys for one node, anywhere, is a
  load-time error — one node, one key.
- **Keys come from trust, never from a hop.** Decision 10 needs the
  destination to hold the origin's public key even with no direct edge.
  In a roster mesh the roster carries every node's key — the nodelist
  carrying identity, as FidoNet's did, signed by the operator. In a pair
  mesh the destination's own pairing with the origin carries it, so
  transit there joins nodes that are paired but have no address for each
  other. A hub never supplies a key, and a paired key in a roster mesh is
  inert (HTTPS-MESH-API.md, "Keys").
- A gate is symmetric or it is nothing: `friends.gates.home = "sakaki"`
  requires `home.gates.friends = "sakaki"`, else `validate` refuses. How a
  destination verifies an origin from the other mesh is an open decision
  (HTTPS-MESH-API.md).

Routing, at the sender and at every hop, for `to.node`:

1. If `to.node` is in `envelope.mesh`, self trusts it there, it is not
   `down`, and self can reach it by its address (or it is `poll` and self
   holds its letters) → spool to it.
2. Else, the `relays` of `envelope.mesh` that self trusts there and can
   reach, not `down`, in declaration order → spool to the first.
3. Else, if `to.node` is not in `envelope.mesh` but in a mesh M for
   which `envelope.mesh` declares a gate G (and M declares G back):
   if self is G, rewrite `envelope.mesh = M` and restart at step 1;
   otherwise reach G by steps 1–2 with G as the target.
4. Else refuse: `no-route` — the sender sees it at send time.

Step 1's mesh clause is the zone wall: a node paired into two meshes
never forwards a letter across them because it happens to know the
destination — only a declared gate rewrites `mesh`, and every hop's
door check (Wire, step 3) verifies the depositing hop belongs to the
zone it claims to carry, and that a rewritten `mesh` is signed by the
declared gate. Every membership, gate, and status lookup in those
checks starts from the **verifying key** and maps it to a declared
name through the roster or the paired record; the `nodes.json`
nickname is a display fact and never an input to policy.

At a hub, a deposit whose `to.node` is not self: run the keyless checks
(§Wire, "Lane and payload"), file the sealed container as `transit`,
append the hub's chained hop signature naming the `next` node the route
picks, re-spool by the four steps. Loops die twice over:
`msgid` seen, and any envelope whose transit chain already names self is
dropped. Deposit `refused` reasons — `no-route`, `down`, `unknown-mesh`,
`zone-violation`, `unverified-origin`, `bad-msgid` — return to the
depositing hop, which records `lastOutcome` on that entry and stops
retrying it; the origin learns through `aoide mail outbox`.

Self-membership uses the node's mesh-declared name; the drift report
flags a box whose local name resolution disagrees with it.

## Delivery and the doorbell

Filing a `letter` to `to.name` rings every **armed** reader of that
name: a session already recorded as a reader (a key in `to.name`'s
cursor map, one of the reader keys `ring_targets` hands back as
`enrolled`) whose latch has not already caught this letter (`rung <=
seq`, `storage::mail`'s own `ring_targets`). A name enrolls before it
rings whenever no enrolled reader is still alive: every live session
whose petname equals `to.name` has its conducted wrap `enrol_reader`-ed
first. A real recorded reader that is still conductable right now is
never second-guessed by a display-name coincidence, so the fallback
never runs out from under a live reader — but a stale enrolment left by
a wrap that read the mailbox once and has since died (no session record
at all, or one whose control socket is gone) never blocks the fallback
either; liveness, not a raw count, is what the ringer checks before
enrolling on a petname match.

Ringing a wrap means more than finding it armed. A ring targets the
**wrap**, a conducted session with a live control socket
(`is_conductable_now`; headless and interactive wraps alike carry one),
but only actually writes once the wrap's **hook-fed agent child** — the
session whose `parentSessionId` is the wrap and whose `agent` names a
registered harness profile, the same "hooked at all" signal
`agent_profile` answers `None` for on an unrecognized one — is sitting
at the prompt (`stopped` or `idle`, `aoide_protocol::state::
canonical_state`). Three ways a target is never written to, in order:
no session record at all for the recorded reader id; recorded but not
conductable right now (including a control socket file that has since
vanished); conductable but with no hook-fed child at all, so there is
no readiness signal to read (`no-readiness-signal`). A target whose
WRAPPED program is a shell (`shell-parent`) is never written to by the
PTY arm either — the line is submitted, so a shell would RUN it — but
that is a transport-level refusal, not a gate: the channel arm below is
not a keystroke and is still taken for such a wrap. A hook-fed child
that IS there but mid-turn (`working`, `awaiting`, or its own `done`
not yet settled) **defers**: the latch stays armed, untouched, and the
next trigger — another letter, or this same reader's own Stop hook
firing once its turn ends — tries again. The filing session's own wrap
is never a candidate for its own letter, not even as a skip or a
defer: it is excluded before the selection ever runs.

Only once a target clears every gate above does transport selection
run. A live Claude Code channel socket (`channel_socket_path`, the
per-session MCP push a stdio session binds for itself) outranks the
wrap's own PTY control socket for **any** wrap that has one,
interactive or headless alike: the channel is a one-way push into the
wrap's own MCP subprocess, never a keystroke, so there is no half-typed
composer line it could ever clobber. The connect is the only test that
matters — a stale socket *file* left behind by an MCP subprocess that
died without unlinking it refuses the connect and falls straight
through, never a stat-only check that would wrongly trust a dead file.
Only once the channel is absent does headlessness matter at all: a
headless wrap with no channel falls back to the PTY exactly as before
P-M5c-3, unless its WRAPPED program is a shell — then it is **skipped**
`shell-parent`, the one refusal that is not about the wrap's own
readiness but about what the line would DO there
(`conduct.rs::wrapped_program_is_a_shell`, the record's own P-C5
capture, never the `agent` label: `--agent <harness> -- bash` is
caught); an interactive wrap with no channel has no transport left and
is **skipped** `interactive-composer` — the same string as before, now
meaning "interactive and no live channel" rather than simply
"interactive". A raw keystroke still auto-submitted into someone's own
terminal remains refused, full stop, with no control-layer guard for
that case (a residual this document still leaves open); what P-M5c-3
adds is a second transport that never types a keystroke at all, so an
interactive wrap running a channel is no longer forced through that
guard to be reached.

A write, once a target clears every check and wins a transport, is
never `session_send` and never gated a second time. Over the
**channel**, it is a single write of the fixed line plus a trailing
newline, flushed, and nothing else — no submit key, ever: the MCP
subprocess on the far end treats the push itself as the delivered
turn, so a keystroke would be a duplicate, not a completion. Over the
**PTY**, it is the same **raw injection** `aoide send` itself uses
(`write_delivery`) — no gate, no pending queue, no provenance prefix,
no title rename — followed, after the short keystroke delay every
conducted delivery already uses, by one submit keystroke, **the
child's own harness key, never the wrap's** (a `kimi` child under a
`claude` wrap submits with `\r`, not `\n`). Either transport, the fixed
line is the only thing that ever reaches the socket:

```
[aoide mail] new mail for <name> — aoide mail read --for <name>
```

A nudge that sits unsent in a prompt is a nudge no one saw, so every
PTY ring is submitted, never left typed; a channel ring needs no submit
at all, since the push itself already stands as delivered. An invalid
`<name>` never rings at all — checked again at the ring, on top of the
check filing already made, before the ring's own lock is even taken;
conduct's own sanitizer separately strips CR/LF from the line
regardless. No byte of the letter itself ever rides the nudge — the
ring's only input is the mailbox name.

A successful write is followed, in order, by a **delivery receipt**
(`file_receipt`, filed from `<name>` to the target wrap's session id —
proof the nudge bytes were written to the wrap's socket, never that
anyone read them) and the **latch stamp** (`stamp_rung`, raising the
reader's `rung` to the arming letter's `seq`). The stamp follows the
write, never precedes it: a write that fails — connect or write
itself, channel or PTY alike, reported `write-failed` either way —
leaves the reader armed, so a target that was merely unreachable for a
moment is not silently skipped forever. A peer that accepts the
connection but never reads is the same failure, not a hang: the write
gives up after a bounded two-second timeout and reports `write-failed`
exactly like any other. The whole select → inject →
stamp sequence
for one name runs under one cross-process critical section — a
dedicated `.ring.lock` file, held across the real socket I/O and the
submit delay, bounded by that same timeout rather than open-ended,
**never** the ordinary stage lock (`try_stage_lock`),
which is only ever held briefly and must never be asked to wait on a
socket. Two concurrent rings for the same name simply serialize on that
file: the second always selects after the first has already stamped,
so a burst of filers rings each armed target exactly once, not once per
filer.

The nudge is **latched**: once rung, a reader does not ring again for
the same name until its own cursor read advances past the point that
rang it. A thousand letters to one name are one line in the reader's
pty, then a count on `aoide mail`; a flood cannot become a thousand
interruptions, and the pty is never the place a flood is felt.

A ring executes only inside the resident daemon: the daemon is the
policy and audit boundary for every ring, not merely a lock holder, so
nothing outside it ever writes to a target's socket. `mail ring --for
<name>` typed at the CLI, or reached through MCP, forwards the request
to the daemon instead of ringing in-process; with no daemon reachable,
nothing rings and the caller sees a reported `"no-daemon"`, not an
error, since the check that matters — is this name valid — still runs
locally first. A reader's own Stop hook replays every name still armed
for the session that just stopped (`armed_names_for_reader`) — the
mechanism that turns a deferred ring, mid-turn, into a delivered one the
moment that turn ends — but only when the daemon is the one handling
that hook; a hook handled locally, with no daemon behind it, replays
nothing and leaves the latch armed for the next daemon-handled trigger.
A remotely deposited letter (the A2A door) arms its readers exactly like
a locally filed one, but does not itself ring anyone: the next
daemon-side trigger for that name — a reader's Stop hook, a local
`mail send`, `mail ring` run by hand — is what rings it, until a later
slice (P-M5b-2) gives the A2A door its own forward path to the daemon.
`aoide-client` sits below `aoide-conduct` in the crate graph and cannot
call the ringer directly, so every one of its callers — `mail send --to
self/<name>`'s own filing path, and `mail ring` itself when reached from
any non-daemon door — forwards through the resident daemon instead
(`daemon_dispatch`).

## Reading

`aoide mail read` renders remote text as data, per house rule 4: each
entry is a fixed header line — `msgid`, `from`, `mintedAt`, `mesh`,
`via` — followed by the text in a fence; `--json` returns the entries
in `Outcome.data`. The reader never interpolates a letter's fields into
anything but that frame, and the frame is built to hold hostile bytes:
header-line fields are clamped to their grammars (`valid_node_name` for
node and name, RFC 3339 for `mintedAt`, hex for `msgid`) with CR, LF and
ESC stripped from anything free-form; the fence is one backtick longer
than the longest backtick run in the text, so no letter can close its
own fence and write outside it. Text is printed raw inside the fence —
a terminal escape in a letter is the reader's terminal's problem to
render inertly, which every modern one does, and the `--json` form
carries the exact bytes for the reader that wants them.

## Boards

A board is every group conversation: a direct board between two
mailboxes (a DM), a project's board, an organization's or a club's
boards. "Board" is the one noun for it. A mailbox stays what a letter
files under and how a board member is named; a thread stays a letter's
reply chain; neither has members or a key. Boards land at P-BOARD.

**Shape.** A board has:

- an **id**: 64 lowercase hex, random at creation;
- one **mesh**: every member node holds `message` in it, and a board
  never spans meshes;
- an **owner**: the node that created it — a project board's owner is
  the project's host node — whose identity key signs every membership
  record; in a roster mesh the operator can move ownership to another
  member node (Takeover, below);
- **members**: mailboxes (`<node>/<name>`). Keys are held per node, so two
  member mailboxes on one node share that node's copy (decision 4);
- a **kind**: `direct` (exactly two members), `project` (hangs off a
  project record), or `group` (any other — an organization's or a club's);
- **`history = all | from-join`**: default `all` for project and group
  boards, `from-join` for direct boards;
- an **epoch** counter. Each epoch has its own age X25519 identity, the
  board's **epoch key**.

**Membership records and wraps.** Every membership change is a
membership record signed by the owner node. Its clear part names the
board, the mesh, the epoch, the epoch key's public recipient and the
member nodes — what a relay needs to admit and route posts. Its sealed
part, sealed to that epoch, names the member mailboxes, the kind, the
title and the `history` setting. The owner sends each member node a
**wrap**: a letter sealed to that node's own age key whose payload is
the epoch key. Nothing else ever carries an epoch key.

- **Create.** `aoide board create` mints epoch 0 and wraps it to every
  member node.
- **Join.** Only the owner adds a member. It wraps the current epoch key
  to the newcomer's node and, when `history = all`, every earlier epoch
  key too. Under `from-join` the newcomer opens nothing sealed before it
  joined. A direct board never grows: adding a third mailbox creates a
  new group board.
- **Removal and leave.** The owner mints a new epoch key and wraps it to
  the remaining member nodes only. Any member may leave at once — its
  node stops filing the board — and the owner's rotation follows. A
  removed member keeps what it already opened and opens nothing sealed to
  a later epoch. A post in flight sealed to the superseded epoch is
  accepted for a bounded grace window, then refused (`stale-epoch`).
- **Rotation is per node.** Removing one mailbox while another member
  mailbox remains on the same node rotates nothing, because that node
  still holds the key. Removing a node from a roster removes its
  mailboxes from every board of that mesh, and each owner rotates.

**Takeover.** In a roster mesh, the mesh's operator can take a board
over when its owner node is dead or gone. `aoide board takeover <board>
[--owner <node>]`, run on the operator's machine, signs a **takeover
record** with the mesh's operator key:

```text
takeover_sig = operator Ed25519 over canonical("aoide/board-takeover", mesh, board, seq, owner_key, epoch)
```

- `owner_key` is the identity key of the new owner, which must be a
  roster node and a member node at `epoch`, the board's current epoch:
  only a member node holds the epoch keys, so only it can read the sealed
  membership and wrap earlier epochs. `--owner` defaults to the
  operator's own machine when that machine is a member node; otherwise
  it is required.
- `seq` counts the takeovers of that board from 1. A member node keeps
  the highest it has accepted and refuses a lower or equal one
  (`stale-takeover`).
- The record travels as a letter to every member node and to the log
  holder. A member node accepts it only under the operator key it trusts
  for the board's mesh (`unknown-operator` otherwise) and only for a
  member node at `epoch` (`not-a-member` otherwise). From then on it
  accepts membership records from the new owner only; one signed by a
  superseded owner is refused (`not-owner`).
- **The new owner rotates the epoch** at once: it mints the next epoch
  key and wraps it to the member nodes except the old owner node, whose
  mailboxes leave the board. The old owner node keeps what it opened and
  opens nothing sealed to the new epoch; if it returns, the new owner
  adds it back like any member. A later joiner receives the earlier
  epoch keys the new owner holds, which under `history = all` is every
  one.
- A board whose log lived on the old owner node (a mesh with no relay)
  is logged on the new owner from then on, starting from the posts the
  new owner has filed.
- A takeover is never silent: `aoide board` marks a board whose owner
  changed by takeover, and each member node audits the record.

A board whose owner node is removed from the roster is frozen until the
operator takes it over. A direct board is never taken over, because the
rotation would leave it one member: without its owner it is frozen, and
the remaining member creates a new one.

**A pair mesh has no operator, so no takeover.** When a board's owner
node in a pair mesh is dead or gone, the board is frozen: its membership
never changes again, its members still read what they filed, and it
carries posts only while its log holder lives. Recreate it: any member
runs `aoide board create` with the same members and owns the new board.
The frozen board's posts stay filed and exported.

**Posts.** A post is a letter on a board. Its envelope is sealed once to
the board's current epoch key (HTTPS-MESH-API.md, "Container": `to.age`
is the epoch recipient, and `ctx` carries the board and the epoch),
origin-signed by the author's node, and delivered to every other member
node. The board's log lives on the mesh's first relay, or on the owner
node when the mesh has none. That node appends each sealed post, fans a
copy out to every member node through the ordinary outbox (`hold` for a
`poll` node), and re-offers the log to a newcomer's node when it joins.
The log holds ciphertext and membership records only; the relay never
holds an epoch key. A member node verifies that the author was a member
node at that epoch, opens the post, files it once as `type=post`, and
rings the board's readers on that node. There is no remote read: every
member reads its own node's copy.

**Reading.** `aoide board` lists the boards with unread posts for this
reader; `aoide board read <board>` prints them and advances the reader's
own cursor, kept in `cursors.json` by board id beside the mailbox cursors
and under the same per-reader rules (§Store), so two readers never
consume each other's posts. Posts render exactly as letters do
(§Reading): a fixed header, the text fenced, no field interpolated
anywhere else. The doorbell line names a board by its id prefix, never
its title, because a title is member-written text.

**Posting from humans and agents.** The conductor's composer (register
§19) and an agent post through the same command, `aoide board post
<board> -- <text>`, under the same daemon policy and audit as `mail
send`. The conductor is a render surface over that command (house rule
7): it paints what `aoide board` reports. A post is data
at rest, never an instruction (decision 2). Whoever wrote it, it never
enters a session's conversation; a reader pulls it on purpose (house
rule 4).

**Letters stay separate.** A letter to a mailbox is not a post on a
direct board:

- The letter is the transport. Posts, membership records, wraps, rosters
  and receipts all move as letters, and a board cannot carry the key
  that opens it.
- Addressed mail needs no membership: a mailbox exists by being named
  (decision 3). Agents write to role and task mailboxes, and a petname
  sender changes every session, so a direct board per mailbox pair would
  give each pair its own key state for no gain: a letter sealed to the
  destination node already reaches exactly the two nodes a direct
  board's key would.
- A direct board is what the User or an agent creates for a
  conversation (`aoide board create --direct <mailbox>`), and `mail
  send` stays how anything reaches a mailbox.

**Project boards.** A project's conversation is its board. The
`project:` addressing target resolves to it (register §15), and posting
there is a board post; `session:` and `role:` targets stay mailbox
resolution. The project board's owner is the project's host node, the
one authority host: membership is decided there, and posts replicate to
member nodes only, never implicitly across the mesh.

**Existing mail.** Letters already in `base.jsonl` stay letters —
immutable, opened, their threads intact. Nothing becomes a board by
migration; boards start empty. `mail export` keeps its thread notes,
and P-BOARD adds one note per board beside them (`type: board`) with the
same fences, clamps and idempotence (§Export).

**Store.** `state/boards/<id>/` holds the board's membership records in
order and the epoch keys this node holds (`0600`; shared with the
board's member nodes by design, never a host key). Posts file in
`base.jsonl`.

## Export

`aoide mail export [--dir <path>]` writes the mailbase out as Markdown,
one note per thread, so the correspondence is searchable from Mneme (register
§30) — before and after the vault share reaches this box. The default is
`$AOIDE_ROOT/state/mail-export/`, the ordinary `state_dir()` resolution, so
the default stays on this box until that share exists; `--dir` goes wherever
the caller says, created if it is missing.

**Read-only on the mailbase.** It is a projection of `base.jsonl` through
`read_base`, and advances no cursor, marks nothing, removes nothing, rings
nothing; a first touch of an unmigrated box runs the same one-shot migrations
every mail command runs (mailbase and cursor shape), which may mint the
identity key. Only `type=letter` entries export — a `receipt` or any other
kind is delivery bookkeeping, not correspondence.

A structured letter with a `threadId` groups by that id, so every copy and
every reply of one conversation lands in one note; a legacy or unstructured
letter — or a structured one with no `threadId` — is its own thread, keyed by
its msgid, the same anchor a legacy reply uses. Letters sit in `seq` order,
and the note is named after the thread key — its first 16 characters when the
key is 64 lowercase hex, else `x` plus the first 16 hex of its sha256: a
stable name, so a re-run overwrites the same file instead of accumulating one
per run, and never the empty stem that would name the hidden `.md`. Two keys
that would name one note refuse the whole run, before anything is written.

```
---
type: mail-thread
thread: "<the full thread key>"
subject: "<the earliest letter's subject, or (no subject)>"
node: "<this box>"
participants: ["<node>/<name>", …]
first: "<received_at>"
last: "<received_at>"
letters: <count>
---

# <subject>

## <received_at> · <from> → <to list>
cc: …

<the letter body, fenced>
```

- **A letter's text only ever rides inside a fence** one backtick longer than
  the longest backtick run in it (three at minimum) — the adaptive fence
  "Reading" gives `mail read`, so no body can close its own fence and write
  markdown. A structured letter contributes its decoded `body`; the encoded
  container the envelope text carries (`AOIDE-LETTER/1` and its JSON) never
  reaches the page, and an invalid or legacy body is fenced verbatim.
- **Everything outside a fence is clamped or quoted.** `received_at` and the
  free-form half of `header.from`/`header.to` (peer-supplied attribution, not
  a grammar-checked name) are stripped of CR, LF and ESC, and every
  frontmatter value a letter or this box's own name supplies — `thread`,
  `subject`, `node`, `participants`, `first`, `last` — is written as a
  double-quoted YAML scalar, so a letter cannot end the frontmatter block or
  forge a heading either. Two values are unquoted, both of them this box's
  own: `type: mail-thread` and `letters: <count>`.
- **Declared recipients win.** The `→` list and the `cc:` line are a
  structured letter's own To and Cc as the sender wrote them; the envelope
  header names only the ONE copy that reached this box. A legacy letter shows
  its envelope's `to`. `participants` is the sorted union of every letter's
  sender attribution and recipient addresses in the thread.
- **One send is one letter.** A `--to`/`--cc` send files one copy per
  mailbox, and each copy is signed separately: its own `sig` and `msgid`, its
  own `minted_at` and `received_at`. Those copies collapse into ONE block —
  the To list and `cc:` line written once, the block's `received_at` the
  earliest copy's — so `letters:` counts letters, not mailboxes, and a send
  to three mailboxes is one letter. Copies are matched on what they share and
  what they are, the signed text, the sender, and the mailbox each copy
  reached; never a timestamp, which two copies of one send can straddle. A
  copy joins only the block directly above it — the one holding the letter
  before it — and only when all three hold: its `seq` is exactly the next one
  after that block's last copy, that block carries the same signed text and
  sender, and that block has not already taken its mailbox. Anything filed in
  between — a receipt, another thread's letter — leaves a gap and ends the
  block, so the same words sent twice to the same mailbox stay two letters,
  not one, and two sends whose single copies land in different mailboxes stay
  two as well. A fan-out whose copies are separated in `seq` by concurrent
  filing renders as more than one block: an over-split is accepted, an
  over-merge is not. Two sends whose copies DO sit back to back are one
  block — nothing in either envelope tells them from one fan-out, and what
  they render is identical.
- **Idempotent.** Each note is written atomically (a temp file in the same
  directory, then a rename), and a note whose bytes already match is not
  written at all: a second run reports `0 written, N unchanged` and leaves
  every inode and mtime alone, so running this on a timer costs nothing.

## Status and the nodelist view

`aoide mesh` is the nodelist command (FTS-5000: "the nodelist defines the
network"). It grows columns: mesh, status (`hold`/`down`/normal), role
(relay/gate/spoke), key source (paired/roster), liveness, and the roster
version in force for each roster mesh. Statuses are declared like every
other mesh fact — in the roster for a roster mesh, in config for a pair
mesh; the command reports, it does not edit. `aoide mail route <node>` runs the four steps and prints
the path without sending — the dry run before a routing change. A later
`aoide mesh down <node>` that edits the declaration for the User is a
convenience allowed by this document, not required by it.

`down` is enforced at the door, in routing (never a hop, never a
destination — `no-route`), and in the drain (skip the directory, drop
entries from a `down` origin). `hold` is ergonomics, not a security
control: it only changes which side initiates.

Two speeds of quarantine, because `down` lives in the declaration and
the declaration is signed by an operator or managed on a NixOS box
(`AOIDE_CONFIG` absolute ⇒ `aoide config set` refuses; a change is a
rebuild, and the rebuild is the User's gate):

- **Now:** `aoide node allow <node> message off` — the existing
  per-node capability switch (per mesh from P-ROSTER, `--mesh`), which
  writes `nodes.json` and which the door reads per request. It narrows a
  roster grant as well as a paired one. The node's deposits refuse on
  the next request; nothing waits for a switch. It is one-sided: this box
  stops listening, routing elsewhere is unchanged.
- **Routing-wide:** `down` for the node in every mesh that declares it —
  the roster's `[status]` and a re-sign for a roster mesh, a config
  `status` line and the next switch for a pair mesh — so every hop
  refuses, routes around, drops. Removing the node's roster line is the
  stronger form: revocation (HTTPS-MESH-API.md, "Rosters").

The door reads `config.toml` and the rosters in force per request for
the mail methods — **new at P-M4**; today the door never loads the
declaration, only `nodes.json`, so this is a seam added, not one reused.
A declaration that fails to load refuses both mail methods with
`config-invalid` and keeps serving everything else: a broken zone table
means no zone checks can run, and no zone checks means no mail, never
"mail with the walls down".

The nodelist view above answers "is this NODE reachable"; `data.delivery`
(both `aoide mail send`'s own post-spool report and `aoide mail outbox`)
answers the narrower, per-entry question "did this LETTER move" — one
shared, read-only join of the outbox entry with its node's own link
state, computed fresh on every call, never cached. Its vocabulary, most
authoritative first:

- `refused` — the entry's own `refused` flag is set (a policy refusal
  the far end sent back, never a transport failure — "Outbox" above).
  Automatic retries have already stopped for this ONE entry, and no
  link state changes that.
- `delivered` — a real, destination-signed ack already sits in this
  box's own mailbase for this exact `msgid` (the ack mechanics: "Wire"
  and "Delivery and the doorbell" above). Never inferred from the
  entry's own absence — an entry can vanish for other reasons too
  (`aoide mail outbox rm`), so absence alone is never read as delivery.
- `accepted` — the peer's own deposit response said accepted or
  duplicate, but no ack has landed yet (`ackPending: true`). A LATER,
  unrelated link failure rides beside it — reason and `nextAttemptAt`
  from the link — without ever downgrading the status: this letter
  already reached the peer, and the link's later trouble is some other
  entry's problem.
- `retrying` — the node's own link state still records an unresolved
  failure; its reason and `nextAttemptAt` (the earliest allowed retry,
  never a promised schedule) ride beside the status.
- `queued` — none of the above: nothing has gone wrong, or nothing has
  been attempted yet.
- `failed` — reserved for a genuine local I/O failure inside `mail
  send`'s OWN drain attempt (never "the remote node was unreachable,"
  which is an ordinary `retrying` outcome). A status READ that fails
  outright instead — a corrupt `link.json`, a concurrently removed
  entry — degrades to `queued` with an explicit "status unavailable"
  reason rather than either an error or a normal-looking queue: the
  spool write already succeeded, or the entry is only being listed, so
  the failure is about REPORTING, never about the mail itself.

## Security model

Threat: code execution as the aoide unix user on one node; also a
paired node that merely misbehaves.

- **On that node: total.** Keys (identity, age, any operator key, the
  epoch keys it holds), ssh, stores, logs. Host security's jurisdiction.
  Mail's duty is to not AMPLIFY it.
- **Across edges, as that node's identity:** only its grants in each
  mesh — `read` (recon), `message` (deposit letters and posts — inert
  data), `spawn`/conduct (gated as ever), transit (carry sealed letters
  within its zones, never open them). Origin signatures stop
  impersonation; sealing stops a hub reading; destination-signed acks
  stop silent-drop-by-forged-ack; trust per mesh and the zone check at
  every hop stop a compromise in one mesh reaching another except
  through a declared gate; `down` is the quarantine gesture and keeps
  the pairing record for forensics; a roster re-sign without the node's
  line is its revocation.
- **As the operator key:** every node that trusts it accepts the rosters
  and board takeovers it signs, until re-rooted — the mesh's root, kept
  on the operator's machine and never on a relay (HTTPS-MESH-API.md,
  "Rosters"). A stolen one can seize the mesh's boards: take them over,
  and re-key a member node's roster line to keys it holds so every
  later wrap reaches it; it opens no epoch wrapped before the re-key.
  Every takeover and every re-keyed line is marked on each member node;
  re-rooting ends it.
- **Across edges, WITHOUT a node's identity:** none. A signed
  connection is verified against one key and that key names the hop;
  an unsigned caller on the loopback is local to the door only when no
  bearer is configured, which is the pre-existing conduct exposure
  named under §Outbox — the mail methods add no unsigned path of their
  own, and refuse any caller that is not a verified node.
- **Not addressed here:** the conduct socket's same-uid fail-open, and
  every other pre-mail surface. This document changes none of them and
  leans on none of them; they are listed so the reader does not assume
  mail's walls extend to them.
- **Prompt injection is the real attack** and decision 2 is the wall:
  attacker text lands as a labeled, provenance-framed entry that a
  reader pulls on purpose. The doorbell carries none of it.
- **Floods** are visible (one audit line per deposit, with origin) and
  bounded only by disk until `down`. A hub amplifies roughly threefold
  per letter — transit entry, outbox copy, returning ack — and
  `seen.jsonl` and outbox directories grow unbounded by ruling. The
  doorbell latch keeps a flood off the pty; the audit log and the disk
  are where it is felt, and `node allow … message off` is the same-
  minute answer. Quotas are addable at the door later without any
  schema change; they are deliberately not in v1.
- **Same-uid honesty**: nothing here is a secret from a local process —
  not the mailbase, not the outbox, not `from.name`, not an epoch key.
- **Several operators, several meshes.** Filing-not-secrecy is an
  on-node ruling; between nodes the design is inter-operator grade.
  The primitives (origin identity, msgid, zones, gates,
  destination-signed acks) were taken from an inter-operator network,
  and the two additions such use needs are both here: sealing to the
  destination key (NNCP's shape, decision 17) and a signed nodelist
  (the roster, decision 18), with trust scoped per mesh (decision 16).

## Commands

```
aoide mail                    names with mail unread by this reader, and its own new letters
aoide mail send --to <node>/<name> [--hold] -- <text>
aoide mail read [--for <name>] [--all-names] [--reread] [--transit]   print + advance cursor
aoide mail show <msgid>                                  one entry, framed
aoide mail mark --for <name>                            advance a cursor without printing
aoide mail outbox [<node>] [rm <msgid>]                  the spool, truthfully, per entry
aoide mail route <node>                                  dry-run the four steps
aoide mail rm --older-than <Nd|Nh>                       prune the base, never seen.jsonl
aoide mail export [--dir <path>]                         one Markdown note per thread (read-only)
aoide mail ring --for <name> [--from <session-id>]       the doorbell, by hand; --from excludes that reader
aoide mail poll                                          a poll node's poll, by hand or from the OS scheduler (H1)
aoide mesh                          nodelist view: + mesh, status, role, key source, liveness, roster version
aoide node allow <node> message off [--mesh <m>]         quarantine this box's door, now (existing command)
aoide identity                                           this node's line: identity key, age binding, fingerprint
aoide mesh join <mesh> (--operator <key> | <operator-node>)   trust a mesh's operator key, once
aoide mesh roster init <mesh>                            root a mesh: mint its operator key (operator's machine)
aoide mesh roster sign <mesh> [--file <path>]            validate, bump version, sign, send to every node
aoide mesh roster show [<mesh>]                          the roster in force: version, signer, this node's grant
aoide mesh roster accept <file>                          apply a signed roster carried by hand
aoide mesh roster reroot <mesh>                          replace a lost or compromised operator key
aoide board                                              boards with unread posts for this reader
aoide board create (--direct <mailbox> | --project <p> | --member <mailbox>…) [--history all|from-join]
aoide board post <board> -- <text>                       post; the conductor's composer runs the same command
aoide board read <board>                                 print + advance this reader's cursor
aoide board add|remove <board> <mailbox>                 owner only; remove rotates the epoch
aoide board leave <board>
aoide board takeover <board> [--owner <node>]            operator's machine, roster mesh only; the new owner rotates
```

`aoide send`, `aoide pair`, `aoide mesh pair` are unrenamed — none of
them lies under the new model. `aoide pair` gains `--mesh`, the pair
mesh the pairing is made in; `aoide mesh pair` converges pair meshes
only, because a roster mesh's nodes need no pairing. `inbox list/read/clear` retire with
their store (golden −3); every new command rides its phase's golden
delta with the full count-site checklist. Like every existing command,
the CLI writes state files itself under the stage lock — "through
aoided" means the policy surface and the audit log, not a socket hop.
Mail writes no audit code of its own. Every CLI dispatch already appends
one line carrying the command, the door, the status and the outcome
message, so a mail command is audited by arriving. A receipt filed at
the two delivery seams adds nothing either: it rides a delivery the log
already recorded, and a second line for one event would make the log
count it twice. A remote deposit is the opposite case — it never passes
the dispatcher and nothing else records it — so `mailDeposit` audits at
the door when P-M2 builds it, which is what makes a flood visible.

**The audit log records that an operation happened, not what it
printed.** A command's outcome message is both its human output and its
audit payload, so a command that renders content copies that content
into the log — for mail, a whole letter on every `mail show`. The log is
bounded instead: `append_audit` clamps the message it stores. Letters
live in the mailbase, which `mail rm` prunes; nothing else keeps a
second copy that pruning cannot reach.

## Phases

Serialized, exec + different reviewer each, cargo field exclusive per
phase, docs in the same commit (this file, CONTRACTS, the crate
READMEs), no subagent spawning and no backgrounded cargo in any brief.

The beta path runs P-M3 → P-SEAL → P-ROSTER → H1 → P-M4 → P-BOARD. H1
(the mail-only HTTPS adapter on the relay) is HTTPS-MESH-API.md's, which
also lists the acceptance tests each of these slices owes beyond its
mail-side tests below. The P-M5 doorbell phases are orthogonal to that
path.

- **P-M1 — the mailbase, locally (M).** `storage::mail`: base/cursors/
  seen under the stage lock, entry + envelope types, canonical header,
  local self-signed envelopes, `msgid`. Commands `mail`, `mail send`
  (to `self/<name>` only), `mail read`, `mail show`, `mail mark`,
  `mail rm`. The two `inbox::receive` seams (`conduct/graph/send.rs`,
  `server/a2a.rs`) repoint to `mail::file_receipt`; `inbox.json`
  migrates on first open; `inbox.rs`, `INBOX_CAP`, `register_inbox` and
  the three `inbox.*` registry entries deleted; CONTRACTS §4 replaces
  the `inbox.json` row with `state/mail/*`. `try_stage_lock` in
  `storage::fs`; migration under it with the atomic rename. Test-root
  helper in test-support. Tests: append-only invariants, torn-tail
  truncation, base-before-seen write order (a `seen` line without a
  base entry is re-accepted), an untakeable lock (an unwritable lock
  path) makes every mail write `Err` and a held lock serializes a second
  writer behind the first, a per-reader cursor advance that leaves a
  second reader's mark untouched, migration mapping (both `inbox.json`
  and a flat `cursors.json`) and a concurrent second opener, seen-set
  survives `rm`, msgid recomputation rejects a tampered field,
  canonical header is byte-exact (case and whitespace change the
  msgid), no fixture escapes the test root. Bare `aoide mail` prints the names half only; its "caller's own
  new letters" half is P-M5c's. Live gate: the timer, door, and CLI
  environments resolve one `state_dir()`.
- **P-M2 — envelopes on the wire, direct edges (L).** Origin signing
  and verification (the pairing keypair), `message` capability,
  `aoide/mailDeposit` in the door (CONTRACTS §6, audit name
  whitelist), the outbox spool with `now` flavor + `link.json` backoff,
  the drain on the timer and in the door process, destination-signed
  ack receipts. `mail send --to <node>/…` for direct edges; `mail
  outbox`; the drain tears its tunnel down on exit. Tests: sign/verify
  round-trip, origin verification is bound to `from.node`'s key (a
  paired node signing as another paired node is `unverified-origin`,
  never filed under the wrong name), duplicate is not an error and a
  duplicate of a filed letter re-spools the receipt, dead-node entry
  waits and drains on return, an ack retires exactly its `msgid` and
  only when signed by `to.node`, a second drain on a busy link skips it
  and leaves the first untouched, an unsigned caller is refused by both
  mail methods, no forward outlives a drain session.
- **P-M3 — hold and poll (M, in progress).** `aoide/mailPoll`,
  `--hold`, the drain's never-attempt rule for hold, poll-on-contact.
  Tests: a hold entry drains only via poll; a poller receives only its
  own entries; a non-`message` or `down` poller is refused; re-poll
  before ack is idempotent.
- **P-SEAL — sealing and key bindings (L).** Each node's age key, minted
  beside its identity key; the self-signed binding, its generation
  high-water mark and retired-key grace; `aoide/binding` on the door and
  the binding in the pairing ceremony; HTTPS-MESH-API.md's container and
  its two verification halves in `mailDeposit`/`mailPoll`; every letter
  that leaves its node sealed at mint, on the SSH direct lane (the only
  transport this slice has), with plaintext only to a destination that
  has published no binding yet. Tests: a sealed letter round-trips over
  the direct lane and files byte-identical to a local filing; the
  outbox and the wire hold no letter bytes; a destination with a binding
  never receives plaintext; a retry resends the stored container
  byte-identical; plus HTTPS-MESH-API.md's P-SEAL list.
- **P-ROSTER — the roster and trust per mesh (L).** Rosters (file,
  `sign`, `accept`, carriage as a `roster` letter, the version high-water
  mark, `reroot`), `aoide mesh join`, the operator line in config, the
  node line from `aoide identity` and `aoide onboard`, grants read per
  mesh at the door (the signed request names its mesh), `nodes.json`
  grants per mesh with every existing record migrated into the home
  mesh, `aoide pair --mesh`, `node allow --mesh` as local narrowing,
  `sameOperator` retired. Tests: a `message`-only roster node deposits
  and is refused `spawn`; a node trusted in mesh A is refused in mesh B;
  a roster letter carried through the relay applies, and a stale one is
  refused; removing a line refuses the node on the next request after
  receipt; every pre-roster grant survives migration byte-identical;
  plus HTTPS-MESH-API.md's P-ROSTER list.
- **H1 — the mail-only HTTPS adapter on the relay.** HTTPS-MESH-API.md.
  Until P-M4, a `poll` node exchanges letters with the relay node itself
  only.
- **P-M4 — zones and transit (L).** `relays`/`status`/`gates` from the
  roster (roster mesh) or config (pair mesh) (validate_mesh:
  multi-membership allowed, key divergence and one-sided gates refused;
  drift shows them), origin verification through the roster or the
  pairing of `originMesh`, the four-step router with the zone clause,
  transit filing of sealed containers + the chained hop signature naming
  `next` + loop guards, the door's per-request declaration read (fail
  closed: `config-invalid`), key→declared-name resolution for every
  policy lookup, `down`/`hold` enforcement at door/route/drain, `mail
  route`, `aoide mesh` columns, the UNREACHABLE vocabulary. Tests on the
  five-edge fixture mesh: osaka→chiyo routes via a relay; an unshared
  mesh gets `no-route`; a dual-member non-gate node refuses to bridge
  (`zone-violation`), and so does a letter whose `mesh` was rewritten
  without a gate's hop signature; a symmetric gate bridges, rewrites
  `mesh`, and its hop signature verifies at the next door; a one-sided
  gate fails validate; a looped envelope is dropped once; `down` refuses
  at all three points; `node allow … message off` refuses on the next
  request with no reload; an unloadable declaration refuses both mail
  methods and nothing else; a renamed `nodes.json` nickname changes no
  policy outcome; a paired key for a node the roster lists is inert.
  Hop chain: a removed middle entry fails `broken-chain`; swapped entries
  fail; **truncating the chain to entry `j` and re-appending the
  truncating hop fails**, on the `prev` or the `next` link; a chain whose
  last `next` is not the destination fails; a chain truncated by dropping
  the tail verifies but yields no ack, and the outbox reports the letter
  undelivered. Plus HTTPS-MESH-API.md's P-M4 list.
- **P-BOARD — boards (L).** §Boards: `aoide board` and its subcommands,
  membership records, wraps, epoch keys under `state/boards/`, posts
  sealed to the current epoch and filed as `type=post`, the board log
  and fan-out on the mesh's first relay (or the owner node), per-reader
  board cursors, the board doorbell line, the `project:` target
  resolving to the project board, board notes in `mail export`, and
  `aoide board takeover` with its takeover record. The conductor's
  board view is register §21's, over `aoide board`. Tests: a
  `history = all` joiner opens every earlier post, a `from-join` joiner
  opens none; a removed member opens nothing sealed after the rotation
  and keeps what it had; a post from a node that is not a member node
  at its epoch is refused (`not-a-member`); a post sealed to a
  superseded epoch past the grace window is refused (`stale-epoch`);
  only the owner's membership record is accepted (`not-owner`); a
  direct board refuses a third member; removing one of two member
  mailboxes on one node rotates nothing; removing a node from the
  roster rotates every board of that mesh it sat on; the relay's
  board log, spool and audit hold no plaintext and no mailbox name; a
  post and its title never reach a doorbell line; a post containing its
  own fence cannot escape the frame; the conductor composer's post and
  an agent's are the same audited command. Takeover tests: a takeover
  under the trusted operator key moves ownership, and the new owner's
  rotation leaves the old owner node unable to open the new epoch; a
  takeover under any other key is refused (`unknown-operator`), so a
  pair-mesh board refuses every takeover; a replayed or lower `seq` is
  refused (`stale-takeover`); a takeover naming a node that is not a
  member node at the current epoch is refused (`not-a-member`); after a
  takeover, a membership record signed by the old owner is refused
  (`not-owner`); a direct board refuses a takeover; removing the owner
  node from the roster freezes its boards (no membership change
  accepted) until a takeover; a later `history = all` joiner under the
  new owner opens every earlier post; `aoide board` marks the board and
  each member node's audit holds the record.
- **P-M5a-1 — the doorbell's latch and readiness floor (S, landed).**
  `storage::mail` gains the latch ahead of the ring itself: `rung` on
  each reader's cursor mark, the `arms(kind)` predicate (a receipt
  never arms), and `ring_targets`/`stamp_rung`/`armed_names_for_reader`/
  `enrol_reader` for a future ringer to call. Mailbox names are
  validated against the node-name grammar at filing, refused rather
  than clamped. No line is injected anywhere in this slice and no `mail
  ring` command exists yet.
- **P-M5a-2 — the ring (S, this phase).** `conduct::graph::doorbell`:
  the actual select → inject → stamp, headless-wrap-plus-hook-fed-child
  targeting, the petname enrol-first fallback, raw injection with the
  child's own submit key, the `.ring.lock` critical section, the
  delivery receipt, and the replay triggers. A ring executes only
  inside the resident daemon: a local self-send and a hand-run `mail
  ring --for <name> [--from <id>]` both forward there from whatever
  door reached them, and a reader's own Stop hook replays whatever is
  still armed only when the daemon is the one handling that hook. A
  remote deposit through `aoide-server`'s A2A door arms its readers the
  same way but does not itself trigger a ring — that door's own forward
  path is P-M5b-2's. Tests: nudge text contains no letter bytes,
  an invalid name is refused and never rewritten, a name with no reader
  and no matching petname rings nothing, the petname fallback enrols
  and rings the conducted ancestor, a live latched reader still blocks the
  petname fallback, every skip/defer reason (not-conductable, an interactive
  composer, no readiness signal, mid-turn), the child's own submit key,
  the filer never rung, one receipt per rung target and never a letter,
  a concurrent burst of filers rings each target once, a failed write
  leaves the latch armed, the Stop hook replays a deferred ring, a
  hundred letters ring once until `mail read`.
- **P-M5a-3 — the interactive composer's PTY guard (S, design first).**
  P-M5c-3 closed the channel-present case: an interactive session with a
  live Claude Code channel socket is rung over that one-way push, never a
  keystroke. What stays open is the raw-keystroke case — an interactive
  composer with no channel is still skipped `interactive-composer`, by
  design, because no control-layer guard exists that makes auto-submitting
  into someone's own terminal safe. This phase designs and lands that
  guard before lifting the PTY-side restriction, if it is ever lifted.
- **P-M5b — fetched receipts.** Whatever this document's "Kill-list" and
  transit design still leave for a delivery-receipt read path once a
  remote reader wants to confirm a letter actually arrived, beyond the
  destination-signed ack P-M2 already provides at the wire layer.
- **P-M5c — polish.** Bare `aoide mail`'s "caller's own new letters"
  half (it needs the reader binding), the reader frame's field clamps
  and adaptive fence, `mesh down` convenience if wanted. Tests: a letter
  containing its own fence cannot escape the frame.

Verification gate per phase: the crate's own tests green, `aoide schema
--json` golden updated in the same commit, and a live two-node run
(yomi ⇄ osaka) of the phase's headline command recorded in the session
ledger. P-ROSTER's live gate is the home mesh rooted and every host
admitted from one signed roster with its old grants intact; H1's is a
native Windows `poll` node exchanging letters with sakaki over HTTPS
with no SSH; P-M4's is the routed letter osaka → sakaki → chiyo, which
needs chiyo lit and the mesh declarations deployed in the ruled order.

## Kill-list

- No mailbox registry, no `mailbox create` — a box exists by being
  named (User ruling).
- No quotas, no eviction, no auto-expiry — dedup + `down` are the whole
  flood story in v1 (User ruling; quotas remain addable at the door).
- No public flag and no public board — every board has members and an
  epoch key.
- No plaintext letter or post in transit — every letter that leaves its
  node is sealed at mint; the direct SSH lane to a destination with no
  binding yet is the one plaintext path, and it never enters a relay.
- No key that opens a letter or a post on a relay, ever.
- No remote read: readers read their own node's base; letters and posts
  travel, readers never do.
- No letter content in any doorbell, notification, or log line — fixed
  text plus a clamped name, ever.
- No transit outside a declared zone except through a symmetric
  declared gate; no implicit bridging by dual membership.
- No key ferried by a hop — keys come from a pairing or from a roster
  signed by the operator key the node trusts, nowhere else; a carrier is
  never the authority.
- No relay of trust, codes, or pairing state — the PAIRING kill-list
  stands; mail transit is the one named carve-out, and a roster riding
  it carries the operator's signature, not the relay's.
- No crowd pairing — one operator's many machines are a roster, never
  one code shown to a room.
- No grant that crosses meshes — trust is per mesh.
- No per-mesh identity — one identity key and one age key per node.
- No cursor sync across nodes — cursors are reader-local, like every
  surveyed system.
- No mail through `message/send` — instruction and data never share a
  wire method.
- No unlocked write to any mail file — every mutation under the stage
  lock, and a write that cannot take it fails rather than proceeds.
- No policy decision keyed on a name the caller wrote or a nickname the
  User chose — the verifying key names the hop, the key on record for
  `from.node` names the origin, everywhere.
- No mail served with the declaration unloadable — `config-invalid`
  refuses both methods until it loads.
