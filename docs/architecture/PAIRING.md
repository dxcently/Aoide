# PAIRING — one identity per instance, one ceremony, permissions off the pair

The design authority for the pairing workstream (absorbs task #47's
remainder: per-peer spawn permission + audit identity). Everything in
"Settled decisions" was User-decided through the 2026-08-25 design
grill; executors do not relitigate it. Where this document and a brief
conflict, this document wins.

## The problem

Connecting two Aoide instances correctly today means hand-wiring up to
three shared-secret credentials per direction per pair, in two
different homes (a token file, the broker), plus `peer add` on both
ends — four verification mechanisms, no keys, no single ceremony. And
the A2A door's spawn arm is origin-blind: any caller presenting the
door-wide bearer may spawn if `aoide.a2a.spawnAgent` is configured;
there is no "this peer may spawn, that one may not."

## Settled decisions

1. **One identity per Aoide instance: a real ed25519 keypair.** No
   hand-rolled cryptography, ever — a vetted, minimal dependency
   (candidates: `ed25519-dalek` 2.x with `rand_core`/`getrandom` for
   key generation; `sha2` for digests — the executor verifies current
   versions and minimal features). This deliberately breaks the
   zero-new-deps discipline; the User authorized the break for
   cryptography specifically, 2026-08-25.
2. **One ceremony: `aoide pair`.** Pairing is THE setup for
   connecting instances, and the ONLY verification. It exchanges
   public keys, marks the peer verified on both ends, and stamps the
   default permissions. The old mechanisms (`token_file`, hand-set
   bearer flags, door-wide bearer for peers) become legacy escapes
   for unpaired callers, documented as such.
3. **Pairing requests ride the wire.** An instance can SEND a pairing
   request to another's door; it parks pending (the same
   park-and-approve idiom as sends and secrets asks). The approver
   confirms via a plain CLI prompt or the desktop popup surface.
4. **UX is a short confirmation code — one per direction, both typed,
   never a bare yes/no on either end (the mutual-code redesign, R1).**
   Both sides independently derive the SAME short code from both public
   keys plus both nonces (numeric-comparison pairing, the Bluetooth
   idiom): the approver types it, read off the requester's screen,
   out-of-band, before committing its own side. The approver then
   derives a SECOND, DIFFERENT short code — the same transcript, plus a
   leading domain-separation tag — and relays it back; the requester
   types THAT code, read off the approver's screen, before committing
   its own side. Two humans, two out-of-band comparisons, one ceremony.
   CLI-first: every step must be drivable from a plain terminal on both
   ends; the popup is sugar.
5. **Per-peer permissions: a closed `allows` set.** Capabilities
   today: `"spawn"` (create a session via the A2A spawn arm),
   `"read"` (graph/who summaries over A2A). Commands:
   `aoide peer allow <name> <cap> on|off` — idempotent, reports
   exactly what changed. Unknown capability strings are refused.
   `autogate` stays its own field (landed semantics, not churned).
   Default for a PAIRED (verified) peer: `config.toml`'s `[pairing]
   defaultGrant`, itself `["read"]` — `spawn` is an explicit widening,
   either standing (`aoide config set pairing.defaultGrant read,spawn`) or
   for one ceremony (`aoide pair --allow read,spawn`).
   An unpaired peer: empty set. Session-level granularity stays open:
   the identity lane (#63) landed the sealed session credential and
   the broker's origin gate, but per-session capability gates (the
   per-surface dispatch-door gates riding the attested-caller lookup)
   are a named, unbuilt remainder — CONTRACTS.md's identity-lane
   accounting.
6. **The spawn gate flips hard.** Spawning requires an identified
   (paired) peer whose `allows` contains `"spawn"`. The door-wide
   bearer alone no longer reaches the spawn arm; the refusal is a
   taught error naming the pairing ceremony. (Inject keeps its
   existing gate — the pending queue + autogate; one gate per
   question.)
7. **Audit identity everywhere the call touches.** The resolved peer
   name is stamped on audit lines, pending-queue entries, and the
   spawned session's record (`origin: "peer:<name>"`, additive),
   surviving into the session ledger.
8. **Mesh law: strictly pairwise.** Cross-mesh reach is a direct
   pair between the two specific boxes. No relay. BINDING CONSTRAINT
   on any future bridge/relay design: every hop carries the true
   origin end-to-end — traced, auditable, never laundered through
   the bridge's own credential.
9. **Everything gates off the one identity.** `allows`, the spawn
   arm, audit provenance now; secrets remote-exposure (the `remote`
   flag) and V5 broker-to-broker replication ride the same pairing
   later — pairing is V5's substrate, not a parallel mechanism.

## Identity

- Keypair minted lazily on first need (`aoide pair` or an explicit
  `aoide identity` command — the executor proposes the command shape),
  stored under the state dir, private key 0600, never printed, never
  in an Outcome. The public key and its fingerprint are freely
  shown.
- The broker is not the keystore for the identity (the daemon/CLI
  must read it without a TOTP ceremony); the identity file follows
  the storage crate's private-file discipline
  (`atomic_write_private` precedent).

## The ceremony

A commit-then-reveal handshake (the Bluetooth SSP idiom), not a single
round trip carrying both sides' pubkeys and nonces in the clear: only the
first mover (A, the requester) commits to its own nonce before revealing
it, so an on-path attacker who observes the request can never control
enough of the SAS transcript to force the same code onto both operators
while sitting between them. B, the approver, may reveal its own nonce
immediately in its synchronous response — A's nonce is already fixed by
A's commitment and stays unknown to anyone until A's own reveal lands two
messages later.

```
box A                                      box B
aoide pair <target> [--name b]
  (a URL dials directly; a bare hostname
   resolves it first, a ~45s discovery
   sweep — see Discovery, below)
  → POST commitment to B's door ─────────► parks pending, UNREVEALED
     (A's pubkey, A's name,                (id, A's pubkey, A's claimed
      commit = H(pubkey_A, nonce_A),        name, origin addr, commit,
      selfVia = A's own reach-back claim)   A's selfVia claim if given)
  ◄── B's pubkey + B's own nonce ─────────┘
  → POST reveal ──────────────────────────► verifies H(pubkey_A, nonce_A)
     (id, nonce_A)                             == commit; stores nonce_A
                                              on match — drops the parked
                                              entry outright on mismatch
both sides now display the SAME SAS, derived from (pubkey_A, pubkey_B,
nonce_A, nonce_B) — B's copy only computable once the reveal landed
                                           operator: CLI prompt on next
                                           bare `aoide pair` /
                                           popup via the events feed
                                           aoide pair [<id>]
B's operator TYPES the code as read off A's screen (out-of-band — a
  phone call, a glance; `--code NNN-NNN` scripted); B compares it against
  its OWN derived SAS, never echoing that SAS in the prompt. A wrong code
  counts a persisted try; the 3rd cumulative mismatch AUTO-DENIES (same
  clean removal as reject, nothing committed).
  On a match, B's own peer record commits HERE — pubkey, verified=true,
  allows=the resolved grant (defaultGrant, or this commit's --allow),
  A's name bound, and (task #131) url/via set
  from A's parked selfVia claim if one rode the request — see below;
  B's own parked entry is marked approved and left PARKED — no
  callback, nothing dials A
A's own `aoide pair` is STILL RUNNING and polling every 5s (task #135 P2,
  up to --wait, default 600s); `aoide pair <id>` is the same
  poll on demand, for a request A detached from (--wait 0) or timed out on
  → POST signed poll ─────────────────────► verifies A's signature against
     (id, timestamp, nonce,                    the pubkey A supplied at
      signature over "PAIRPOLL"+id+...)        request time; approved? →
  ◄── B's pubkey, IF approved ─────────────┘   release; else → "pending"
A's outbound entry now sits at awaiting-confirm (visible in bare `aoide
pair`'s row as that state — never a code itself, P-PV2); B, having
already approved, derives a SECOND, DIFFERENT code from the SAME
transcript (a leading domain-separation tag makes it un-collidable with
the first) and relays it back to A's operator out-of-band; A's own
operator TYPES that reply code in the SAME `aoide pair <id>` invocation
that just polled — or in A's still-running `aoide pair`, which reaches
the same confirm-and-commit — never a bare yes/no acknowledgment of the
code A's own screen already showed. A wrong reply code counts a
persisted try the same way B's leg does; the 3rd cumulative mismatch
AUTO-ABORTS the outbound entry. ONLY on a match does A's own peer record
commit. A timeout leaves the request parked, so the two paths are
interchangeable.
`aoide pair reject <id>` on A's outbound entry aborts at any point
before that confirm, with no wire call and no record on either end.
```

The poll (`aoide/pairPoll`) replaces an earlier design's reverse callback
outright (Design A, task #119): B's door used to dial OUT to A the moment
its operator approved, which meant a requester whose own door binds
loopback-only could never be reached and the ceremony could never
complete. Now nothing ever dials IN to A — A polls B over the SAME forward
dial its own `request`/`reveal` already used, so pairing works end to end
even when A's door accepts no routable connection at all.

- **`aoide pair watch --popup` is opt-in** (`aoide.a2a.pairingPopup`, nix
  option, default off) and shows ONE dialog shape on BOTH pairing
  directions now (the mutual-code redesign, R1), never a direction-shaped
  split and never a bare yes/no on either. Either direction collects the
  typed code the same way the CLI tty prompt and the scripted `--code`
  path do, through the identical gate — a six-boxes-plus-dash entry
  surface, `lyra pair ask` when `lyra` resolves, `zenity --entry`
  otherwise — and never shows the code, same as the tty prompt: inbound
  against `derive_sas`, outbound against its own second, different reply
  code (`derive_reply_sas`) rather than the code this instance already
  generated and showed itself. This reverses the OUTBOUND dialog's
  original shape (P-PV3, task #132) — a plain Approve/Reject over its own
  already-displayed code — because that code failed the retype-is-theater
  test only for a value already on the SAME screen; the reply code comes
  from a genuinely different one. Both entry dialogs share a
  `"Reject request"` rejection control; a bare Cancel/Escape leaves the
  request offered again next tick; either binary failing to spawn backs
  off the retry cadence rather than silently dropping the request. **No
  pairing dialog closes on a timer (R2, the mutual-code redesign's popup
  phase).** The entry dialog and the reply-code display dialog below both
  sit open until the operator answers, the request is resolved elsewhere,
  a live blocking `aoide pair` claims the same id, or Ctrl-C interrupts
  the watcher itself. No timeout and no re-offer cadence exist: a
  genuine answer is worth waiting for, and a single-operator desktop
  only ever has one modal question open at a time regardless. The
  accepted cost: while a dialog sits open, the
  watcher's own feed narration and outbound poll both pause. **After a
  popup-driven INBOUND commit succeeds, the approver's own reply code gets
  a SECOND, separate dialog** — `lyra pair show` (`zenity --info
  --no-markup` otherwise), the code shown large with a Copy control and a
  Done control, no reject control at all: it fires only after the commit
  it belongs to already succeeded, so there is nothing left to approve or
  reject, only to relay out-of-band and dismiss. The same commit also
  fires a `notify-send` toast carrying the same code, first and
  independently of the dialog — a persistent glance-backup that still
  lands the code when `lyra`/`zenity` are both missing. The watcher polls
  outbound entries still `awaiting-approval` on its OWN 60s timer (through
  the same poll-once seam the blocking command uses), which is what lets a
  detached (`--wait 0`) request's entry dialog ever fire; the 200ms popup
  tick itself never makes a network call. And a LIVE blocking `aoide pair`
  holds a pid marker for its id (`$XDG_RUNTIME_DIR/aoide/`, the
  tunnel-record convention) that suppresses the watcher's dialog for that
  id — two surfaces never race one commit; a marker naming a dead pid is
  stale, cleaned up, and suppresses nothing.
- **`aoide pair` takes exactly ONE positional, and its two subcommands win
  over a same-named hostname target.** A second positional — old `peer
  pair request <url>` muscle memory is the one that bites — is refused
  outright, before either the URL or hostname arm ever runs, naming the
  fold in the refusal itself; `pair reject`/`pair watch` are
  registered subcommands and match BEFORE a bare hostname positional of
  the same literal spelling, so a box actually named `reject` (or
  `watch`) cannot be paired by that bare name — the explicit URL
  form always works.
- The short authentication string (SAS) is derived from a transcript
  hash over both public keys + both nonces — SHA-256 over the four
  fields, lowercased/trimmed/NUL-separated, truncated mod 1,000,000
  (`aoide_storage::pairing::derive_sas`; the exact derivation and its
  pinned vectors live in CONTRACTS §6's "Pairing wire" subsection —
  standard SAS construction, no invention). `derive_reply_sas` (R1) is a
  SECOND derivation over the identical transcript plus a leading
  domain-separation tag — B's own reply code, never the same code
  A already computed — same subsection, its own pinned vectors. The
  human SAS comparison, on EITHER code, is THE gate against an active
  on-path attacker; the poll is retrieval, never a second source of
  trust — it releases nothing until BOTH ends have already committed to
  the SAME transcript.
- **Bare `aoide pair` never shows the SAS (P-PV2, the User's locked
  spec).** The code is read off the REQUESTER's own screen and typed on
  the APPROVER's — printing it in a listing either operator can glance at
  would collapse that out-of-band comparison into a copy exercise. Each
  side's own `aoide pair` independently re-derives it, exactly as
  above; bare `aoide pair`'s rows carry the id, direction, name, and state — never the code.
  A target names the request to act on: bare `aoide pair` lists, and
  never approves on its own even when exactly one request is pending —
  approving is always an explicit act (the id or name on the command
  line, or a pick from the tty menu).
- A pairing request that is never approved (or never confirmed on A's
  own side) expires (timeout knob, default generous — hours, not
  minutes; it waits for a human, twice). An APPROVED-but-not-yet-polled
  entry expires the same way — it stays parked, never removed early,
  until either the poll releases it or the ordinary timeout sweeps it —
  with one exception (R3, below): a fresh request from the SAME
  requester pubkey supersedes it early, on the theory that the
  keyholder retrying IS the requester abandoning its own ceremony.
- **The two ends commit asymmetrically, on purpose.** B's peer record
  for A exists the moment B's own operator approves; A's peer record
  for B exists only once A's own operator polls-and-confirms afterward,
  over a SECOND, DIFFERENT code (R1) — B's own reply code, never the
  first code shown twice. A never-confirmed A simply leaves B holding a
  verified peer that answers nothing — visible on B's own `peer status`,
  resolved by an ordinary expiring re-pair, never a silent one-sided
  pairing. This is decision 4's mutual confirmation carried all the way
  through: a code shown once and accepted once was never actually a
  MUTUAL confirmation, only a promise that the other side would
  eventually agree.
- **B's commit records a peer A can actually be reached at, even through
  a tunnel (task #131).** A door reachable only through ssh (the
  loopback-only case Transport's own section below covers) means every
  `aoide/pairRequest` B ever sees from A arrives over A's own tunnel — B
  can only OBSERVE the connection as loopback, and nothing about the
  connection itself says how to dial A back. So the ceremony's OWN dial
  (A's `aoide pair`, either arm) rides the SAME derived `via` its
  record always got — no longer only the record, per Transport's own
  section — and A's request carries an OPTIONAL `selfVia` claim (`ssh://
  [user@]host`, defaulting to A's own `$USER`/`$LOGNAME` login at the
  LOCAL OUTBOUND ADDRESS routed toward B — never a claimed OS hostname,
  which this LAN proved resolves only through the router's DHCP-DNS —
  overridable via `--self-via`): A's own self-asserted answer to "how do
  you reach me back," the same trust class as the `url` field beside it
  (self-asserted data, a transport marker only — trust stays in pubkeys +
  SAS, never either field). When B's parked entry carries that claim,
  `aoide pair`'s commit sets the resulting peer's `url` to
  `http://127.0.0.1:<port>/` (loopback-as-seen-from-the-far-side — the
  sakaki/chiyo/osaka rows in a live `peers.json` are this exact shape),
  where `<port>` is A's OWN door port, parsed off A's `url` on the parked
  entry (never B's own `AOIDE_A2A_PORT`, which names nothing about A —
  only a fallback when that parse itself fails), and `via` to the claim
  itself, in the same write as the pairing commit. No claim — an old A, or
  one with nothing to claim — commits exactly the shape B's commit always
  produced: `url` is A's advertised door verbatim, `via` stays unset.
- Re-pairing an existing peer replaces the key material only after
  the same confirmation — never silently.
- The inbound park queue is capped (`AOIDE_PAIRING_PARK_CAP`, default
  32) — an unauthenticated door refusing to park indefinitely, the same
  discipline the secrets broker's own ask-park queue holds. Outbound
  entries are operator-created, one per `aoide pair` invocation,
  and carry no cap.
- **One live parked request per requester identity (R3): SUPERSEDE, not
  refuse.** A fresh `aoide/pairRequest` from the SAME `pubkeyHex` evicts
  whatever this identity already has parked — approved-but-unpolled
  included — rather than coexisting with it or refusing outright; the
  eviction runs BEFORE the cap check, so a retry never burns cap
  headroom. The match is case-insensitive: the same canonicalization
  `transcript_digest` already applies to every field before hashing, so
  two hex-case variants of the same key already commit and reveal
  identically and must supersede as one requester. `aoide pair`'s own
  outbound queue mirrors this: `park_outbound` replaces by id OR by the
  approver's own pubkey, the same case-insensitive match, one live
  outbound entry per far identity. A cross-direction pair — an inbound
  request FROM X alongside an outbound request TO X — is a legitimate
  simultaneous mutual pairing and is left alone. Supersede widens who
  can evict a pending request from "on-path" to "knows the pubkey" —
  the same accepted denial-of-one-attempt class CONTRACTS.md §6 already
  accepts for a bogus reveal, never an impersonation — and holds only
  because the A2A door binds loopback by default (this document's own
  Transport section, below): reaching `pairRequest` at all already
  requires a shell on the box or an ssh tunnel into it, and that reach
  already grants a direct read of the parked file itself. Bound
  routably instead, that condition no longer holds and refuse-the-second
  becomes the safer default (CONTRACTS.md §6's own park-cap paragraph
  carries the full statement).
- **The poll authenticates without a peer record.** `aoide/pairPoll`
  cannot use the paired-peer signed-header scheme below — no verified
  peer record exists yet for the id being polled — so A signs a
  self-contained message (the SAME canonical-string primitive, a
  different method label) directly against the pubkey B already
  captured at request time. An unauthenticated or wrongly-signed poll
  gets the identical answer a not-yet-approved one gets — never a
  distinct signal an outsider could use to learn whether an id exists.

## Wire authentication (paired peers)

Per-request detached signature replaces bearer comparison for paired
peers:

- Headers: peer name (attribution only — see below), timestamp, nonce,
  signature. Signature is
  ed25519 over a canonical string binding method, path, timestamp,
  nonce, and the body digest (`sha2`). The executor writes the exact
  canonical form into CONTRACTS §6 in the same commit that lands it.
- Replay guard: timestamp window (±120s default) + a bounded nonce
  cache. Clock skew beyond the window earns a taught error naming
  the skew.
- Unpaired callers keep today's door-wide bearer path (read arms
  only, per decision 6). Fail-closed discipline mirrors #84
  (sentinel on resolve failure, constant-time comparisons where
  secrets are still compared).
- **A verified signature outranks loopback for the Inject delivery
  gate.** A loopback-terminating transport hop — an ssh `-L` forward
  is the case that motivates this — delivers a remote peer's request
  from ITS OWN end's loopback, so the connection's observed origin
  classifies as loopback exactly like a genuinely local caller. A
  request whose signature this door already verified is, by
  construction, never a local caller, so it is treated as remote for
  the auto-deliver-vs-pending question regardless of which address it
  arrived from: `state/peers.json`'s per-peer `autogate` flag, not
  connection origin, decides whether a signed peer's send still
  auto-delivers. An unsigned request's loopback trust is unaffected —
  this narrowing only ever removes a free pass a signature was never
  entitled to in the first place.
- **Identity IS the key; the name is a label (#63 P-ID5).**
  `X-Aoide-Peer` carries the caller's own self name
  (`aoide_storage::display::local_host_name()`), the same value the
  pairing wire's `pairRequest.name` sends — never the caller's local
  nickname for the counterpart — but it does no identity work: the far
  end resolves the caller BY THE KEY THAT SIGNED, trying the signature
  against every verified peer's stored pubkey and taking the record
  whose key verifies. The name is display/attribution — a
  claimed-vs-resolved mismatch is audited as attribution drift and the
  resolved name wins everywhere downstream — so renaming a peer
  locally never breaks inbound signed requests from it. Nothing
  name-trusted remains on the signed path; the name's one residual
  role is the exact-name tiebreak when multiple verified records share
  the verifying pubkey (equal proven key, possibly different grants —
  no exact-name match refuses as ambiguous rather than guessing).
  A signature matching no verified peer's key refuses identically to a
  bad signature — never an existence oracle over the registry.
  CONTRACTS §6's "Inbound verification" carries the pinned check
  order, collision semantics, and the pubkey-keyed nonce cache.

## Phases

Serialized, Sonnet exec + different-Sonnet review each, cargo field
exclusive per phase. Golden count changes ride the same commit with
the full count-site checklist (git show 9c2d05c).

- **P-P1 — identity + the dependency (M).** The vetted dep lands
  (workspace-level justification comment in the Cargo.toml), keypair
  mint/load/store in `storage` (or a new small crate if storage's
  charter resists — executor states the reading), `aoide identity`
  show command (golden +1). Tests: mint-once idempotence, 0600, pubkey
  round-trip, no private material in any Serialize type (grep gate).
- **P-P2 — pairing wire + CLI ceremony (L).** Request/park/approve
  over the A2A door (new method, CONTRACTS §6), SAS derivation +
  display, `peer pair request|pending|approve|reject` commands (golden
  +N), peer record gains `pubkey`/`verified` (additive serde),
  legacy fields untouched. Tests: full ceremony against a test door,
  SAS stability vectors, expiry, re-pair confirmation, unapproved =
  no record change.
- **P-P3 — allows + the spawn gate flip + audit identity (M).**
  `allows` set + `peer allow` commands (golden +1), spawn arm requires
  paired+spawn (taught error otherwise), audit/pending/session-record
  stamping (`origin: peer:<name>`, projected into the ledger).
  Tests: gate table (paired+allowed / paired+denied / unpaired /
  legacy-bearer), stamp presence end-to-end, defaults per decision 5.
- **P-P4 — signed requests (L).** The wire-auth scheme above for
  paired peers; unpaired callers unchanged. Tests: signature
  round-trip, replay rejection, skew taught error, canonical-string
  vectors pinned.
- **P-P5 — pairing events feed + the popup watcher (S).** `a2a serve`
  emits `pair-parked`/`pair-revealed` onto aoided's own events feed
  (`pair-awaiting-confirm` shipped with this phase but is dormant since
  task #119 retired its emitter) — the same seam secrets asks already ride
  (`class: "gate"`, `source: "a2a-door"`, by-name payload, no SAS/
  pubkey/nonce/commitment ever on the wire, CONTRACTS §6's "Pairing
  events feed" subsection). The phase adds a `watch` subcommand (later
  folded into bare `aoide pair watch` at task #135 P3') that tails the
  feed and re-derives the actionable set from `aoide-storage::pairing`
  directly (golden +1); `--popup` ships zenity `--question` only at this
  phase — no `lyra` fallback yet, a QML dialog named as a deferral, built
  out at P-PV3 below. Legacy-escape documentation pass lands as CONTRACTS
  §6's own "Legacy escapes" subsection.
- **P-P6 — discovery + invite (M).** The LAN advertisement
  (advertise-off-by-default, emitted by `a2a serve`), `peer discover`
  + `peer invite` (golden +2, full count-site checklist),
  advertisement validation as untrusted input, CONTRACTS §6
  advertisement format + constants. Tests: advertisement round-trip
  over a real loopback socket, malformed advertisement dropped,
  bounded dedupe, invite resolves a heard advertisement and refuses
  an unheard/ambiguous name, advertise switch default-off, discovery
  writes nothing to the registry.
- **P-PV1 — reach-back through a loopback-only door (M, task #131).**
  The requester-side `via` derivation (P-P6's discovery-observed-address
  default) only ever solves HALF the tunnel problem — it lets the
  requester dial the approver. The approver's OWN commit still needed the
  SAME answer in the opposite direction: every `aoide/pairRequest` a
  tunneled requester sends arrives looking like loopback, with no
  observed address the approver could derive a `via` from. `selfVia`
  closes it — an OPTIONAL, self-asserted `ssh://[user@]host` field the
  requester rides on `aoide/pairRequest` (login defaults to `$USER`/
  `$LOGNAME`, host defaults to the LOCAL OUTBOUND ADDRESS routed toward
  the approver, `--self-via` overrides both). Present on the parked
  entry, `peer pair approve`'s commit rewrites the resulting peer's `url`
  to `http://127.0.0.1:<port>/` — loopback-as-seen-from-the-far-side,
  `<port>` parsed off the REQUESTER's own advertised door port, never
  this box's own `AOIDE_A2A_PORT` — and sets `via` to the claim itself, in
  the SAME write as the pairing commit; absent, the commit is exactly the
  pre-P-PV1 shape (`url` verbatim, `via` unset). The ceremony's own dial
  (both `peer pair` arms) also starts riding the SAME derived `via` a
  discovered peer already gets, not only the record. Tests: loopback
  record correctness with and without a claim, the requester's own port
  parsed off the right field, the ceremony's dial itself using the
  derived `via`.
- **P-PV2 — the collapsed command surface (M, the User's locked spec,
  three grill rounds).** `peer pair request`/`peer invite` DIE outright
  (hard cutover, no aliases) and fold into ONE smart-target `aoide peer
  pair <target>`: a URL-shaped target (`"://"`) dials directly; anything
  else resolves by discovery sweep (default 45s, not the 4s
  `peer discover`/`peer invite` used to share — task #129's known miss).
  Both arms reuse the SAME `run_pair_request` core the pre-P-PV2 commands
  called (golden −1: two dead paths, one new). `peer pair pending`
  RENAMES to `peer pending` (golden net 0) and its rows drop the SAS —
  the code stays purely out-of-band, read off the requester's screen and
  typed on the approver's. `peer pair approve`'s `<id>` becomes optional
  when exactly one request is pending. Review follow-up in the same lane:
  `peer pair` refuses a SECOND positional outright rather than silently
  reading only the first — old `peer pair request <url>` muscle memory
  would otherwise land `request`/`<url>` as `peer pair`'s own two args,
  past its single declared target, burning a full sweep window hunting a
  host literally named "request" while discarding the url unremarked.
- **P-PV3 — the popup's typed-code upgrade, opt-in (M, task #132).**
  `peer pair watch --popup`'s INBOUND (approver) dialog stops being a bare
  Approve/Reject: it collects the typed code through the SAME
  six-boxes-plus-dash surface the secrets TOTP dialog already has
  (`lyra pair ask`, sharing that command's own QML component via the new
  `dialog_qml` module, `zenity --entry` as the fallback) and runs it
  through the identical `CodeGate::Code` gate the CLI already holds — no
  new no-prompt gate variant (`InboundGate::DialogConfirmed` is retired)
  — and never shows the code in the dialog. The OUTBOUND (requester)
  dialog originally kept a plain confirm shape (`lyra pair confirm`/
  `zenity --question`): this instance's own locally-derived code shown
  large, a single Approve/Reject committing unconditionally — a
  design-review round within this SAME task had already caught an
  earlier pass collecting a retype on this arm too (reusing the inbound
  entry surface with the code pre-shown as context) as copy-the-pixels
  theater, since the code was already on screen in the same window the
  retry field sat in. That confirm shape held only until the mutual-code
  redesign (R1) gave the outbound leg a genuinely SECOND code (the
  approver's own reply code, arriving from a different screen) to gate
  on — at which point the SAME retype-is-theater argument this task's own
  review round established points the other way: a value from a
  different surface is exactly the case that argument named as real
  typed entry, not theater. `run_ask_dialog` now runs on BOTH directions;
  `run_confirm_dialog` and the plain-confirm outbound shape are gone.
  Deployed behind a NEW flag, `aoide.a2a.pairingPopup` (default false) —
  the unit's desktop-facet gate is unchanged, this flag is the deliberate
  opt-in on top of it, modules' own "flags default off" house rule.
- **Task #135 P3' — the one-command collapse (M, "the command set can just
  be `aoide pair`").** `peer.pair`, `peer.pair.approve`, `peer.pair.
  reject`, `peer.pair.watch`, and `peer.pending` DIE outright — hard
  cutover, no aliases, same as `peer.invite` before them. Bare `pair`
  (already registered) becomes the ONE command: with no target it is the
  pending listing (interactive menu over pending requests plus heard
  advertisers on a real CLI tty, the JSON-friendly listing off a tty /
  with `--json` / on a non-CLI door); with a target, an exact
  pending-request-id match wins over a name match, a pending inbound
  request from the target is approved, a pending outbound one is
  resumed, and anything else starts a new request. `pair.reject` and
  `pair.watch` register as the two remaining subcommands of `pair`. The
  `peer` family keeps the roster (add/list/allow/hub/spawn/pull/status/
  discover/advertise); `pair` mints the verified records those operate
  on. Golden count 82 → 79.

Live gates at the end of the lane: a real pair between yomi and
sakaki via the ceremony (codes compared on real terminals), a spawn
refused for a peer with spawn revoked, a spawn admitted and its
ledger entry carrying `origin: peer:<name>`.

## Discovery (advertise-but-locked)

User-decided 2026-08-25: on a large network Aoide finds other Aoide —
but discovery grants NOTHING. Connecting still requires the pairing
ceremony; discovery only tells you who is there to invite.

- **Transport: Aoide's own UDP broadcast advertisement** (User-chosen
  over mDNS — zero new dependencies, cargo-only, nix-independent).
  One JSON line per advertisement to the limited-broadcast address on
  a fixed port (constants pinned and documented in CONTRACTS §6):
  `{v, name, host, user}` — protocol version, instance name, and the
  advertiser's ssh hop claim (the makings of `--via ssh://user@host`).
  Never a credential, never a key or fingerprint (rendezvous, not
  authentication — pairing's SAS stays the trust gate), never a door
  URL (doors are loopback-bound; ssh is the only cross-box
  transport). Broadcast replaced the original multicast group (task
  #120, the #106 fix): cross-box multicast was eaten by the User's
  router — verified live 2026-08-27, each box heard only itself — and
  a LAN this size needs none of multicast's efficiency. A broadcast
  needs no group membership on either end: the listener is a plain
  `0.0.0.0` bind on the fixed port, no interface pinning, no
  capability probing.
- **Advertising is OFF by default.** The advertisement is emitted by
  the `a2a serve` process, and a box advertises only when told to:
  `aoide peer advertise on|off` flips `state/advertise.json`
  (idempotent, reports what changed), which the advertise thread
  reads every tick — a toggle lands within one ~30s jittered cadence,
  no restart; `AOIDE_DISCOVERY_ADVERTISE`/`aoide.a2a.
  discoveryAdvertise`/`--discovery-advertise` force it on for a
  process's lifetime (the nix-declarative path), OR'd with the
  switch. No resident listener exists: hearing is on-demand.
- **A host must let the advertisement's UDP port through its own
  firewall.** A default-deny firewall (NixOS's stock one trusts only
  `lo`) drops an inbound datagram on a physical interface before any
  Aoide socket ever sees it — task #98's diagnosis, confirmed by an
  independent raw UDP send/receive that succeeds over `lo` and fails
  identically over the physical interface, with no Aoide code in the
  path at all. The nix module opens the port automatically whenever
  `aoide.a2a.discoveryAdvertise` is on, since that flag is already
  the operator's deliberate opt-in to being found on the LAN; a box
  that only ever wants to receive (advertising off, running `peer
  discover`/`aoide pair` against others) must open that port itself.
- **`aoide peer discover [--secs N]`** listens briefly (default a few
  seconds), dedupes by (name, source address) into a BOUNDED
  in-memory fold (a hostile flood of fabricated names is counted
  dropped past the cap, never grown), and prints the table: name, the
  claimed ssh hop `user`@`host`, the observed source address,
  first/last heard. The same heard-set is what `aoide peer list`
  (CONTRACTS §7's CLI surface) merges into its mesh roster — a heard
  name matching a registered peer marks that row advertising (`●◆`/
  `○◆`), an unknown name becomes a `◆` pair-candidate row, and a
  self-heard advertisement only marks this host's own row.
  An advertisement is UNTRUSTED NETWORK DATA (house
  rule 4's discipline): every field is validated (name shape, bounded
  metacharacter-free host, POSIX login shape, line-size cap) before
  display, a malformed advertisement is dropped and counted, and
  nothing is ever written to the peer registry by discovery alone.
- **The claimed hop is a claim; the observed source address is a
  fact.** `host`/`user` are whatever the advertiser typed onto the
  wire. The UDP packet's own source IP, by contrast, is measured
  directly by the socket that heard it and cannot be spoofed the way
  a self-reported field can. `peer discover` shows both, side by
  side, precisely so the two can be seen disagreeing; anything that
  dials uses the OBSERVED address, never the claimed one.
- **`aoide pair <name>`'s hostname arm** is sugar over the ceremony,
  nothing more: any target that isn't URL-shaped (no `"://"`) runs its
  own discover sweep, resolves `<name>` to the matching advertisement
  (ambiguous or absent name = taught error listing what WAS heard),
  composes a dial target from that advertisement's OBSERVED source
  address on the house door port (`AOIDE_A2A_PORT` or 8710 — the wire
  carries no port to read; a far end on a non-default port takes the
  explicit URL-target arm, `pair <url>`), shows the claimed hop and
  the observed address, and then runs the SAME `run_pair_request` flow
  the URL arm runs against the composed target — recording a `via`
  derived from the observed address plus the claimed login for the
  resulting peer's future calls. One ceremony stays the only
  verification.
- **`pair`'s hostname arm refuses to pair with yourself.** A
  broadcast always loops back to its own sender, so a box that advertises
  hears itself every sweep. Before dialing anything, it checks whether
  the resolved target is this instance's own advertisement: the heard
  name matching this instance's own, or the datagram having come from
  loopback. Either one is a taught refusal, never a ceremony run
  against yourself. KNOWN GAP: a serve advertising under a custom
  `--peer-name` (flag only — the env/hostname tiers agree on both
  sides) escapes the name arm, and the self-heard broadcast arrives on
  the physical interface so the loopback arm misses too — `aoide pair
  <own-custom-name>` will dial this box's own door and park a
  self-pairing request. Confusion, not compromise: both SAS codes land
  in front of the same operator, and the ceremony commits nothing
  without their approve.
- **Spoofed advertisements are phishing, and the ceremony catches
  them.** An attacker advertising a victim's name can lure an invite
  — but the SAS confirmation is mutual: the code on the inviter's
  terminal must match the code on the REAL counterpart's terminal,
  and the counterpart's operator must approve. An advertisement can
  misdirect a request; it cannot survive the code comparison. Two
  sources claiming one name in the same sweep is refused as
  ambiguous before either is dialed.

## Transport

Every A2A door stays loopback-bound, always — that invariant does not move.
What moves is HOW a request reaches it from another box: an internal ssh
forward, opened lazily by `aoide-client` and dialed through instead of the
peer's own host directly. The tunnel is a TRANSPORT hop, not a protocol
relay — the signed peer identity (Wire authentication, above: the key that
signed) still crosses it end to end, and the far door verifies the exact same
request it always did. Nothing about §"Settled decisions"'s "strictly
pairwise, no relay, every hop carries the true origin" changes; an ssh `-L`
forward is a pipe, not a party to the protocol.

- **A peer dials directly unless a `via` marker says otherwise.** `Peer.via`
  (`ssh://[user@]host[:port]`, CONTRACTS.md §7) is absent by default — every
  peer registered today, and every peer this ceremony creates without an
  explicit `--via`, dials `url` exactly as before. Explicit beats implicit:
  a `--via` flag on the command itself always outranks a peer's own
  recorded `via`.
- **The dial's authority changes; its path never does.** When a `via` IS in
  play, the client rewrites the dial url's `scheme://host:port` to
  `http://127.0.0.1:<local port>` — the internal forward's own local end —
  but copies the PATH verbatim from the logical url. Wire authentication's
  signature is computed over the path, never the host, so this rewrite
  changes no byte of what gets signed or what the far end verifies.
  `aoide_storage::tunnel::dial_url` is the one place this cut happens,
  deliberately sharing `peer_store::url_path` with the signer rather than
  re-deriving it, so the two can never drift apart.
- **`pair`'s hostname arm/bare `pair` derive a `via` automatically, and
  (task #131) the ceremony's OWN dial rides it too.** Discovery's own
  observed source address (this document's Discovery section, above) is
  only ever reachable directly when the advertising box's door itself
  binds somewhere routable — against a loopback-only door (this section's
  own opening invariant), a direct dial to that address never connects at
  all. So `pair`'s hostname arm/bare `pair` derive a `via` from the
  observed address plus the advertisement's claimed ssh login, and that
  SAME default now drives BOTH halves: the ceremony's own two POSTs dial
  through it, and it is recorded on the resulting peer once pairing is
  approved, so that peer's FUTURE calls (pull, spawn, send) have a working
  transport marker too. The one exception is an advertisement with no ssh
  claim at all (empty login) — nothing to tunnel through, so the dial
  stays direct, the same shape it held before task #131. An explicit
  `--via` on `pair`, either arm, overrides this for both the
  ceremony's own dial and the recorded marker, exactly as before.
- **The APPROVER'S side needs the SAME answer, from the OTHER direction —
  this is what `selfVia` is for (task #131).** Everything above describes
  the REQUESTER deriving a `via` to reach the approver. But when the
  requester's OWN door is loopback-only, the approver faces the identical
  problem in reverse: every `aoide/pairRequest` it receives arrives over
  the requester's tunnel, so the connection LOOKS like loopback no matter
  where the requester actually is — there is no observed address to derive
  anything from. `aoide/pairRequest` carries an OPTIONAL `selfVia` field
  for exactly this: the requester's own self-asserted `ssh://[user@]host`
  claim of its own reach-back hop — the LOGIN half defaults to
  `$USER`/`$LOGNAME`, and the HOST half defaults to the LOCAL OUTBOUND
  ADDRESS routed toward the peer being dialed (`UdpSocket::connect` picks
  a route with no packet sent; a claimed OS hostname was tried first and
  found to resolve only through the router's DHCP-DNS on this LAN —
  resolution by luck, not something a transport marker can lean on),
  falling back to the claimed hostname only if that route lookup itself
  fails; `--self-via` overrides the whole default. Same trust class as the
  `url` field beside it on that same wire message either way:
  self-asserted data, a transport marker only, never itself a source of
  trust (trust stays in pubkeys + the SAS comparison, "The ceremony"
  section above). `aoide pair`'s commit reads it: present, the
  resulting peer gets `via` set to the claim and `url` rewritten to
  `http://127.0.0.1:<port>/`, where `<port>` is parsed off the
  REQUESTER's own `url` (their real door port — never this box's own
  `AOIDE_A2A_PORT`, which names nothing about the requester), falling back
  to `AOIDE_A2A_PORT`/`8710` only when that parse itself fails; absent, the
  commit is unchanged from before task #131.
- **Ssh keys are the substrate; aoide never manages them.** The client
  spawns `ssh -N -T -o BatchMode=yes …` — no password or host-key prompt
  can ever appear, so a box missing the far side's key in its
  `authorized_keys` fails fast with a taught error naming the one-time
  manual step, never hangs waiting on a prompt nothing here could answer.
  Aoide never writes to anyone's `authorized_keys`; a trusted home network
  making ssh keys an acceptable substrate is the User's own premise for
  this lane, not something this code decides on his behalf.
- **A tunnel is reused within a session, never held open forever.** Opened
  lazily on first use (`aoide_client::tunnel::open_or_reuse`), keyed by
  `(session id, peer name)`, reused by every later action under the same
  conducted session. A clean `session end` closes every tunnel it opened as
  its own fast path (`aoide_client::tunnel::close_all_for_session`,
  `aoide-conduct::graph::session_store::do_session_end`); `aoide-conduct::
  reap::sweep_orphan_tunnels` is the SUPER+Q/SIGKILL backstop for a session
  that never got to run that exit path — a roster-less, settled record's
  still-answering `ssh -N` child is signaled
  (`aoide_client::tunnel::kill_if_still_our_ssh`) before its record is
  unlinked, the same shape the reaper already holds for a killed session's
  control socket. Nothing about a tunnel is ever allowed to become a
  resident daemon. A bare shell with no conducted session gets a
  process-scoped tunnel instead — per-command rather than persistent.
- **The door's own loopback trust narrows so a tunneled request cannot
  silently auto-deliver.** Every tunneled connection reaches the far door's
  socket as loopback — that is what a forward IS. The door treats an
  UNSIGNED loopback connection as implicitly trusted for Inject delivery (a
  sound stance behind a bind that only a genuinely local caller could ever
  reach); a tunnel is a way to reach that same socket from somewhere else,
  so a request carrying a verified per-request signature never rides that
  trust — it is a remote peer by construction, and `a2a.rs::origin_for_
  inject` strips loopback's free pass from it before the delivery decision
  runs (CONTRACTS.md §6). A signature-rung `autogate` flag restores
  auto-delivery for a peer the operator already marked that way, exactly
  the like-for-like an operator's existing grant expects. `via`/`--via` are
  safe to use against a real peer.

## Mesh declaration

A named mesh (`config.toml`'s `[mesh.<name>]`, task #135 P4, CONTRACTS.md
§4) is intent, not a second identity model. It is an operator's own
bookkeeping — "these are the boxes I expect to belong to this roster,
reached at these hops" — recorded once, on one instance, never transmitted:
nothing in the ceremony, the wire (§"Wire authentication" above), or any
A2A payload carries a mesh name, and `peer_store::Peer` gains no field for
it. The roster itself stays exactly what the Kill-list below already
settled — the closure of pairwise, individually-verified records — and a
declared mesh only ever describes a NAMED EXPECTATION over that same
closure, never a new object standing in front of it. Declaring one changes
nothing about how a peer is paired, verified, or reached.

`aoide mesh` (`aoide_client::mesh`) is the read side: it compares a
declaration against the live registry and reports where they diverge — a
declared peer with no record at all, one whose pairing was never confirmed,
or one whose recorded `via` no longer matches the declared hop (an absent
`via` there is the severe case, since a call then dials the peer's bare
address directly — commonly this box's own loopback, for an already-paired
peer). It also lists any verified peer that belongs to no declared mesh,
reported plainly rather than treated as a problem. Every finding is a
report; nothing about running it pairs, re-pairs, or edits a single record.

### The converge

`aoide mesh pair [<mesh>]` is the write side, and it writes nothing this
document does not already describe: it runs the ceremony above, unchanged,
once per peer. It reads the SAME comparison `aoide mesh` renders — there is
no second one — and selects the declared peers with **no verified record**:
`missing` and `unverified`, in declared-name order. Each is dialed at
`http://127.0.0.1:<door port>/` through its declared hop, which is the
ordinary tunneled dial every cross-box peer action already makes. The
result is ordinary pairwise records and nothing else — no transitive relay,
no mesh-level object, nothing new on the wire; the Kill-list below holds
unchanged.

**A verified peer is never modified.** A `via-mismatch` comes back
`skipped`, naming `aoide pair <name>` as the fix, for three reasons: a
re-pair replaces key material and is never silent, the re-pair confirm
already exists as a human y/N on `aoide pair <name>`, and `via` is written
by a ceremony commit alone — a converge writing it would be a second writer
of a field this document reserves to that commit. It also means a converge
is idempotent: run it twice and the second run touches nothing. The
direction matters when fixing one: the requester's commit is what records
`via`, so a wrong hop is repaired by a ceremony started FROM the box holding
the wrong record.

**Nothing pairs with nobody watching.** A converge is a loop over the
ceremony, so every leg keeps both typed codes: the far operator reads the
pairing code and types it, then reads a reply code back. What the converge
adds in front is ONE pre-flight confirm covering the whole run — which
peers, in what order, through which hops, at what grant — which `--yes`
skips exactly as it skips the sweep's proceed-prompt on `aoide pair`. It is
a convenience over the listing, never a substitute for a code: skipping it
bypasses no gate, because the codes are the gate.

**`sameOperator` is declared and not acted on.** Whether a converge may
ever satisfy the far side's typed code on an operator's behalf — a claim of
one human at both screens — touches the mutual-code invariant directly and
is not decided. Until it is, a mesh declaring `sameOperator = true`
converges byte-identically to one that does not, and the report carries a
single note saying the flag was seen and not acted on. The note is a note:
it changes no peer's outcome, no count, and refuses nothing.

`mesh.<name>.grant` IS live: it is the capability set a converge stamps at a
first verification, riding the ceremony exactly as a typed `--allow` does,
so an already-verified peer's `allows` survives a re-pair untouched and
`peer allow` remains the only way to change a live grant.

Each peer's result is one of four words — `completed`, `parked` (resumable
by id), `UNREACHABLE`, `skipped`. `UNREACHABLE` is separate from `parked`
because a request only parks once both ceremony POSTs succeed: a request to
a box that is off parks nothing, so there is no id to resume and the report
must not offer one.

Two boxes converging the same declaration at once needs nothing new: A→B
and B→A at the same moment is exactly the cross-direction pair R3 above
already calls a legitimate simultaneous mutual pairing and leaves alone.

## Kill-list

- No hand-rolled cryptography — primitives come from the vetted dep,
  full stop (User ruling).
- No daemon-door or A2A allowlist tables — the per-command door policy
  and the `allows` set are the only gates (AOIDED L2 discipline).
- No transitive relay, no mesh-level object — the mesh stays the
  closure of pairwise records.
- No per-capability serde bool scatter — `allows` is one field.
- The identity private key never crosses any socket, any Outcome,
  any log — including the broker's.
