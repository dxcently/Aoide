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
writers — `inbox list/read/clear` can show it, but nothing can DEPOSIT
into it by address. Sessions cannot leave each other letters. Every
FidoNet node could.

## Settled decisions

1. **Mail first; boards later.** Mail is addressed, one-to-some,
   store-and-forward. Boards (public, area-flooded, echomail-shaped) are a
   separate later phase on the same store, `aoide board`, cribbed from
   NNCP multicast areas rather than raw FidoNet.
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
4. **Filing, not secrecy.** Every agent on a node is the same unix user;
   any intra-host lock would be fiction. A name says who a letter is
   *for*, never who may read it. FidoNet netmail was sysop-readable; so
   is this.
5. **Deposits are ungated for paired nodes.** Pairing is the trust
   (per-link, FidoNet's pkt password made real). A new `message`
   capability joins the closed `NODE_CAPABILITIES` vocabulary; a verified
   node holding it deposits without per-letter approval. Conduct's gate
   (`pending.json`, autogate) is untouched and mail never queues there.
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
    start and the User's `down` gesture, disk absorbs it. The audit log's
    one line per deposit, with origin, is the detection.
15. **Declared node status: `hold` and `down`.** In the mesh config,
    per node. `hold` queues at the sender silently; `down` refuses fast
    and stops routing to/from that node. The UNREACHABLE-vocabulary
    ruling lands in the same change.
16. **Meshes are zones.** Trust is per edge and flat (`nodes.json`); the
    mesh declaration partitions ROUTING, not trust. A node may be declared
    in several meshes; declaring it with differing `grant`s is a
    load-time validation error, never a silent first-wins.

## Vocabulary

| word     | meaning                                                            | FidoNet ancestor            |
| -------- | ------------------------------------------------------------------ | --------------------------- |
| node     | a mesh member: one Aoide instance, one keypair                     | node                        |
| edge     | a verified pairing between two nodes; where grants live            | link, pkt password          |
| mesh     | a named `[mesh.<name>]` declaration; a routing zone                | zone                        |
| hub      | a node a mesh declares as a transit relay (config key `relays`)    | hub / host                  |
| zonegate | a node both of two meshes declare as their gate to each other      | zonegate                    |
| name     | free-text recipient name inside a letter                           | `toUserName`                |
| letter   | an envelope of type `letter`                                       | netmail message             |
| envelope | the signed, immutable unit that moves: header + text               | packed message              |
| msgid    | hash of the signed envelope; global, transfer-invariant            | `^AMSGID` (as NNCP MsgHash) |
| mailbase | a node's append-only store of entries                              | message base                |
| outbox   | the sender-side per-node spool                                     | BSO flow files              |
| flavor   | `now` (deliver or queue+retry) / `hold` (wait to be polled)        | `?ut` flavors               |
| cursor   | a reader's high-water mark over the mailbase                       | `.newsrc`                   |
| receipt  | an envelope of type `receipt`: a conduct-delivery record or an ack | ReturnReceipt               |

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
- Each transit hop signs `msgid ‖ 0x00 ‖ node ‖ 0x00 ‖ at ‖ 0x00 ‖ mesh`
  with its own key — the `mesh` it is carrying the letter in is inside
  that hop signature, which is what makes a gate's rewrite verifiable.
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
msgid, mesh, transit: [ {node, at, sig} ] }` — the receiver re-derives
the canonical header from the fields and rejects if `msgid` does not
recompute.

## Store

`$AOIDE_STATE_DIR/mail/` (the ordinary `state_dir()` resolution —
`AOIDE_STATE_DIR` first, then `AOIDE_ROOT/state` — like every other state
file):

```
base.jsonl      append-only, one entry per line, immutable once written
cursors.json    { "<name>": { "<reader>": { "seq": n } } }
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
- `type=transit` entries are letters the node is relaying (§Transit);
  readers hide them unless asked.
- Reading never mutates an entry. Cursors are per reader: a name maps to
  a set of readers, each with its own high-water mark, and a read
  advances only the caller's. Two agents sharing a mailbox never
  consume each other's mail. A reader is identified by its conducting
  session id (`AOIDE_SESSION_ID`), and by the mailbox name itself when
  that is unset — so a stray read from an unconducted terminal advances
  a pseudo-reader and never a live agent's mark. A respawned session is
  a new reader and sees the name from the beginning, the way an NNTP
  client with no `.newsrc` does: re-delivery is the safe direction, and
  silent loss is not. The reader set is also what the doorbell rings.
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
  filed locally is the same bytes it would be on the wire. `<name>` is
  free text; it is filed raw and **clamped only when rendered into a
  pty**.
- **Address role names, never session petnames.** Petnames are minted
  `adjective-noun` per session and change on every respawn; a role name
  (`rebuild-reports`, `conductor`) outlives the session that reads it.
  A brief says "report to `yomi-strix/conductor`", not to a petname.
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
  ∈ `accepted | duplicate | refused(reason)`. Policy, in order:
  1. caller is a verified node holding `message`, not `down`. "Verified"
     means a connection signature that checked against ONE key; the
     hop's identity for every later step is the node that key belongs
     to — never a name the caller wrote in the envelope, never a
     `nodes.json` nickname looked up by name;
  2. `msgid` recomputes from the envelope;
  3. **zone check:** the receiver declares `envelope.mesh`, and the hop
     from step 1 is declared in it. If `envelope.mesh ≠ header.originMesh`
     the letter has crossed a gate, and the last `transit` element must be
     a hop signature (§The envelope) over this `mesh` from the declared
     gate between the two meshes — else `zone-violation`. A dual-member
     node cannot re-label a letter into its other mesh: its hop signature
     would name a node that is no gate;
  4. origin signature verifies against **the key on record for
     `header.from.node`** — `nodes.json` when paired, else the mesh
     declaration's key table (§Transit). One key is tried, the one that
     name maps to. Today's connection-signature resolution tries every
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
  outbox entry retires only when a receipt arrives whose verified signer
  is the entry's `to.node` and whose text names the entry's `msgid`.**
  A hub can mint a receipt; it cannot sign as the destination, so it
  cannot make an origin stop retrying.

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
  the two drains from racing one link.
- A drain session opens the node's ssh tunnel, deposits, and **tears the
  tunnel down before it exits** — never leaves a forward standing for
  the next tick. A standing `ssh -L` is a loopback path into the far
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
  A `refused` outcome stops that entry's retries and records the reason.
- A `down` node's directory is skipped entirely; entries whose ORIGIN is
  `down` are dropped at the next drain.
- An entry retires only on a valid ack (above) or `aoide mail outbox rm
  <msgid>`. `aoide mail outbox [<node>]` answers "did it land"
  truthfully per entry: waiting, held, tries, last outcome, acked.

## Transit

Additive keys in the mesh declaration (deny_unknown_fields — the same
deploy-order rule as the mesh keys themselves: **every box switches
before any declaration uses them**; the drift report is the dry run):

```toml
[mesh.home]
grant  = ["read", "spawn", "message"]
relays = ["sakaki", "yomi-strix"]        # transit hubs for THIS mesh

[mesh.home.nodes]
osaka = "ssh://khoa@192.168.1.201"
# …

[mesh.home.keys]                         # ed25519 pubkeys, hex — the nodelist
chiyo = "…"                              # carries identity for nodes you do
                                         # not pair with directly

[mesh.home.status]                       # absent = normal
chiyo = "hold"

[mesh.home.gates]                        # cross-mesh transit, by mesh name;
work = "osaka"                           # the OTHER mesh must declare it back
```

- **A node may be declared in several meshes** (P-M4 relaxes today's
  cross-mesh uniqueness check in `validate_mesh`); differing `grant`,
  or differing declared keys, for one node across those meshes is a
  load-time error — one node, one key, one grant, wherever it appears.
- **Keys are declared, not ferried.** Decision 10 needs the destination
  to hold the origin's public key even with no direct edge. The key
  table is written by the User (the nodelist carrying identity, as
  FidoNet's did); a hub never supplies one. For a directly paired node
  the paired key is authoritative and drift reports a declared key that
  disagrees with it.
- A gate is symmetric or it is nothing: `home.gates.work = "osaka"`
  requires `work.gates.home = "osaka"`, else `validate` refuses.

Routing, at the sender and at every hop, for `to.node`:

1. If `to.node` is declared in `envelope.mesh` and self holds a direct,
   verified, not-`down` edge to it → spool to it.
2. Else, the `relays` of `envelope.mesh` that are direct verified
   not-`down` edges of self, in declaration order → spool to the first.
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
name through the paired record or the key table; the `nodes.json`
nickname is a display fact and never an input to policy.

At a hub, a deposit whose `to.node` is not self: verify (all six
steps), file as `transit`, append the hub's hop signature to
`envelope.transit`, re-spool by the four steps. Loops die twice over:
`msgid` seen, and any envelope whose transit chain already names self is
dropped. Deposit `refused` reasons — `no-route`, `down`, `unknown-mesh`,
`zone-violation`, `unverified-origin`, `bad-msgid` — return to the
depositing hop, which records `lastOutcome` on that entry and stops
retrying it; the origin learns through `aoide mail outbox`.

Self-membership uses the node's mesh-declared name; the drift report
flags a box whose local name resolution disagrees with it.

## Delivery and the doorbell

Filing a `letter` rings the doorbell for every live local session that
is a recorded reader of `to.name` (a key in that name's cursor map) —
and, for a name no one has read yet, for any live session whose petname
equals it.
The doorbell is a fixed line injected through the loopback conduct path
(no gate — local `aoided` conducting a local session):

```
[aoide mail] new mail for <name> — aoide mail read --for <name>
```

`<name>` is clamped to the `valid_node_name` grammar (`[a-z0-9-]`,
`node_store.rs`) before injection; conduct's own sanitizer only strips
CR/LF, which is not a clamp. No byte of the letter rides the nudge.
Headless spawns get it — injection is the path conduct uses. Nothing
else is ever injected on a letter's behalf.

The nudge is **latched**: it rings once per (name, session) and not
again until that reader's cursor for the name advances. A thousand
letters to one name are one line in the reader's pty, then a count on
`aoide mail`; a flood cannot become a thousand interruptions, and the
pty is never the place a flood is felt. The line is injected and
submitted, the way a conducted `--submit` is — a nudge that sits unsent
in a prompt is a nudge no one saw.

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

## Status and the nodelist view

`aoide mesh` is the nodelist command (FTS-5000: "the nodelist defines the
network"). It grows columns: status (`hold`/`down`/normal), role
(relay/gate/spoke), key source (paired/declared), liveness. Statuses are
declared in config like every other mesh fact; the command reports, it
does not edit. `aoide mail route <node>` runs the four steps and prints
the path without sending — the dry run before a routing change. A later
`aoide mesh down <node>` that edits the declaration for the User is a
convenience allowed by this document, not required by it.

`down` is enforced at the door, in routing (never a hop, never a
destination — `no-route`), and in the drain (skip the directory, drop
entries from a `down` origin). `hold` is ergonomics, not a security
control: it only changes which side initiates.

Two speeds of quarantine, because `down` lives in the declaration and
the declaration is managed on a NixOS box (`AOIDE_CONFIG` absolute ⇒
`aoide config set` refuses; a change is a rebuild, and the rebuild is
the User's gate):

- **Now:** `aoide node allow <node> message off` — the existing
  per-node capability switch, which writes `nodes.json` and which the
  door reads per request. The node's deposits refuse on the next
  request; nothing waits for a switch. It is one-sided: this box stops
  listening, routing elsewhere is unchanged.
- **Routing-wide:** `status.<node> = "down"` in every mesh that declares
  it — every hop refuses, routes around, drops — landing with the next
  switch, and recorded in git like every other mesh fact.

The door reads `config.toml` per request for the mail methods — **new
at P-M4**; today the door never loads the declaration, only
`nodes.json`, so this is a seam added, not one reused. A declaration
that fails to load refuses both mail methods with `config-invalid` and
keeps serving everything else: a broken zone table means no zone
checks can run, and no zone checks means no mail, never "mail with the
walls down".

## Security model

Threat: code execution as the aoide unix user on one node; also a
paired node that merely misbehaves.

- **On that node: total.** Keys, roster, ssh, stores, logs. Host
  security's jurisdiction. Mail's duty is to not AMPLIFY it.
- **Across edges, as that node's identity:** `read` (recon), `message`
  (deposit letters — inert data), `spawn`/conduct (gated as ever),
  transit (relay letters within its declared zones). Origin signatures
  stop impersonation; destination-signed acks stop silent-drop-by-
  forged-ack; the zone check at every hop stops a compromise in one
  mesh reaching another except through a declared gate; `down` is the
  quarantine gesture and keeps the pairing record for forensics.
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
  not the mailbase, not the outbox, not `from.name`.
- **Not a multi-operator design** — filing-not-secrecy and files-at-hub
  are same-operator rulings. The primitives (origin identity, msgid,
  zones, gates, destination-signed acks) are inter-operator grade
  because they were taken from an inter-operator network; end-to-end
  encryption to the destination key (NNCP's shape) and nodelist
  distribution are the two additions such a use would need. This
  document forecloses neither.

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
aoide mesh                          nodelist view: + status, role, key source, liveness
aoide node allow <node> message off                      quarantine this box's door, now (existing command)
aoide board …                       reserved; not in this workstream
```

`aoide send`, `aoide pair`, `aoide mesh pair` are unrenamed — none of
them lies under the new model. `inbox list/read/clear` retire with
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
  survives `rm`, msgid recomputation rejects a tampered field, canonical header is byte-exact
  (case and whitespace change the msgid), no fixture escapes the test
  root. Bare `aoide mail` prints the names half only; its "caller's own
  new letters" half is P-M5's. Live gate: the timer, door, and CLI
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
  only when signed by `to.node`, two drains on one link serialize on
  `.bsy`, an unsigned caller is refused by both mail methods, no
  forward outlives a drain session.
- **P-M3 — hold and poll (M).** `aoide/mailPoll`, `--hold`, the drain's
  never-attempt rule for hold, poll-on-contact. Tests: a hold entry
  drains only via poll; a poller receives only its own entries; a
  non-`message` or `down` poller is refused; re-poll before ack is
  idempotent.
- **P-M4 — zones (L).** `relays`/`keys`/`status`/`gates` keys
  (validate_mesh: multi-membership allowed, grant or key divergence and
  one-sided gates refused; drift shows them), declared-key
  verification, the four-step router with the zone clause, transit
  filing + hop-signature chain + loop guards, the door's per-request
  declaration read (fail closed: `config-invalid`), key→declared-name
  resolution for every policy lookup, `down`/`hold` enforcement at
  door/route/drain, `mail route`, `aoide mesh` columns, the
  UNREACHABLE vocabulary. Tests on the five-edge fixture mesh:
  osaka→chiyo routes via a relay; an unshared mesh gets `no-route`; a
  dual-member non-gate node refuses to bridge (`zone-violation`), and
  so does a letter whose `mesh` was rewritten without a gate's hop
  signature; a symmetric gate bridges, rewrites `mesh`, and its hop
  signature verifies at the next door; a one-sided gate fails validate;
  a looped envelope is dropped once; `down` refuses at all three
  points; `node allow … message off` refuses on the next request with
  no reload; an unloadable declaration refuses both mail methods and
  nothing else; a renamed `nodes.json` nickname changes no policy
  outcome; a declared key disagreeing with a paired key is drift.
- **P-M5 — doorbell + polish (S).** The fixed-line nudge through
  loopback conduct with the `valid_node_name` clamp, submitted, latched
  per (name, session) until the cursor advances; reader-recorded
  targeting, bare `aoide mail`'s "caller's own new letters" half (it
  needs the reader binding), headless-session coverage, the reader
  frame's field clamps and adaptive fence, `mesh down` convenience if
  wanted. Tests: nudge text contains no letter bytes, adversarial
  names (ESC, `$(`, `;`,
  newlines) clamp to the grammar, a name with no reader and no matching
  petname rings nothing, a hundred letters ring once until `mail read`,
  a letter containing its own fence cannot escape the frame.

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
  text plus a clamped name, ever.
- No transit outside a declared zone except through a symmetric
  declared gate; no implicit bridging by dual membership.
- No key ferried by a hop — keys come from pairing or the User-written
  key table, nowhere else.
- No relay of trust, codes, or pairing state — the PAIRING kill-list
  stands; mail transit is the one named carve-out.
- No per-mesh identity — one keypair per node.
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
