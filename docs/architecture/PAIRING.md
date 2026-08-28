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
2. **One ceremony: `aoide peer pair`.** Pairing is THE setup for
   connecting instances, and the ONLY verification. It exchanges
   public keys, marks the peer verified on both ends, and stamps the
   default permissions. The old mechanisms (`token_file`, hand-set
   bearer flags, door-wide bearer for peers) become legacy escapes
   for unpaired callers, documented as such.
3. **Pairing requests ride the wire.** An instance can SEND a pairing
   request to another's door; it parks pending (the same
   park-and-approve idiom as sends and secrets asks). The approver
   confirms via a plain CLI prompt or the desktop popup surface.
4. **UX is a short confirmation code.** Both sides display the same
   short code derived from both public keys (numeric-comparison
   pairing, the Bluetooth idiom); the human confirms the codes match
   before the pair is committed. CLI-first: every step must be
   drivable from a plain terminal on both ends; the popup is sugar.
5. **Per-peer permissions: a closed `allows` set.** Capabilities
   today: `"spawn"` (create a session via the A2A spawn arm),
   `"read"` (graph/who summaries over A2A). Commands:
   `aoide peer allow <name> <cap> on|off` — idempotent, reports
   exactly what changed. Unknown capability strings are refused.
   `autogate` stays its own field (landed semantics, not churned).
   Default for a PAIRED (verified) peer: `["read", "spawn"]`.
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

- Keypair minted lazily on first need (`peer pair` or an explicit
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
aoide peer pair request <url> [--name b]
  → POST commitment to B's door ─────────► parks pending, UNREVEALED
     (A's pubkey, A's name,                (id, A's pubkey, A's claimed
      commit = H(pubkey_A, nonce_A))        name, origin addr, commit)
  ◄── B's pubkey + B's own nonce ─────────┘
  → POST reveal ──────────────────────────► verifies H(pubkey_A, nonce_A)
     (id, nonce_A)                             == commit; stores nonce_A
                                              on match — drops the parked
                                              entry outright on mismatch
both sides now display the SAME SAS, derived from (pubkey_A, pubkey_B,
nonce_A, nonce_B) — B's copy only computable once the reveal landed
                                           operator: CLI prompt on next
                                           `aoide peer pair pending` /
                                           popup via the events feed
                                           aoide peer pair approve <id>
B's operator TYPES the code as read off A's screen (out-of-band — a
  phone call, a glance; `--code NNN-NNN` scripted); B compares it against
  its OWN derived SAS, never echoing that SAS in the prompt. A wrong code
  counts a persisted try; the 3rd cumulative mismatch AUTO-DENIES (same
  clean removal as reject, nothing committed).
  On a match, B's own peer record commits HERE — pubkey, verified=true,
  allows=["read","spawn"], A's name bound; B's own parked entry is
  marked approved and left PARKED — no callback, nothing dials A
aoide peer pair approve <id>  (A polls, whenever A gets around to it —
  → POST signed poll ─────────────────────► verifies A's signature against
     (id, timestamp, nonce,                    the pubkey A supplied at
      signature over "PAIRPOLL"+id+...)        request time; approved? →
  ◄── B's pubkey, IF approved ─────────────┘   release; else → "pending"
A's outbound entry now shows the SAME SAS a second time (`peer pair
pending`, state awaiting-confirm); A's own operator confirms it y/N
(A's screen printed this code itself at request time — the typed-code
gate is B's side) in the SAME `peer pair approve <id>` invocation that
just polled — ONLY THEN does A's own peer record commit.
`aoide peer pair reject <id>` on A's outbound entry aborts at any point
before that confirm, with no wire call and no record on either end.
```

The poll (`aoide/pairPoll`) replaces an earlier design's reverse callback
outright (Design A, task #119): B's door used to dial OUT to A the moment
its operator approved, which meant a requester whose own door binds
loopback-only could never be reached and the ceremony could never
complete. Now nothing ever dials IN to A — A polls B over the SAME forward
dial its own `request`/`reveal` already used, so pairing works end to end
even when A's door accepts no routable connection at all.

- `peer pair watch --popup`'s dialog is the one approver surface that
  does NOT take the typed code: clicking Approve on the rendered code is
  that arm's whole confirmation (the dialog IS the gate there), and a
  typed-code entry field in the dialog is a named follow-on. The CLI tty
  prompt and the scripted `--code` path are where the typed-code gate
  lives.
- The short authentication string (SAS) is derived from a transcript
  hash over both public keys + both nonces — SHA-256 over the four
  fields, lowercased/trimmed/NUL-separated, truncated mod 1,000,000
  (`aoide_storage::pairing::derive_sas`; the exact derivation and its
  pinned vectors live in CONTRACTS §6's "Pairing wire" subsection —
  standard SAS construction, no invention). The human SAS comparison is
  THE gate against an active on-path attacker; the poll is retrieval,
  never a second source of trust — it releases nothing until BOTH ends
  have already committed to the SAME transcript.
- A pairing request that is never approved (or never confirmed on A's
  own side) expires (timeout knob, default generous — hours, not
  minutes; it waits for a human, twice). An APPROVED-but-not-yet-polled
  entry expires the same way — it stays parked, never removed early,
  until either the poll releases it or the ordinary timeout sweeps it.
- **The two ends commit asymmetrically, on purpose.** B's peer record
  for A exists the moment B's own operator approves; A's peer record
  for B exists only once A's own operator polls-and-confirms afterward,
  over the SAME code. A never-confirmed A simply leaves B holding a
  verified peer that answers nothing — visible on B's own `peer status`,
  resolved by an ordinary expiring re-pair, never a silent one-sided
  pairing. This is decision 4's mutual confirmation carried all the way
  through: a code shown once and accepted once was never actually a
  MUTUAL confirmation, only a promise that the other side would
  eventually agree.
- Re-pairing an existing peer replaces the key material only after
  the same confirmation — never silently.
- The inbound park queue is capped (`AOIDE_PAIRING_PARK_CAP`, default
  32) — an unauthenticated door refusing to park indefinitely, the same
  discipline the secrets broker's own ask-park queue holds. Outbound
  entries are operator-created, one per `peer pair request` invocation,
  and carry no cap.
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
  events feed" subsection). `aoide peer pair watch [--popup] [--json]`
  tails it and re-derives the actionable set from `aoide-storage::
  pairing` directly (golden +1); `--popup` is zenity `--question` only
  — no `lyra` fallback, a QML confirm dialog is a named deferral, not
  built. Legacy-escape documentation pass lands as CONTRACTS §6's own
  "Legacy escapes" subsection.
- **P-P6 — discovery + invite (M).** The LAN advertisement
  (advertise-off-by-default, emitted by `a2a serve`), `peer discover`
  + `peer invite` (golden +2, full count-site checklist),
  advertisement validation as untrusted input, CONTRACTS §6
  advertisement format + constants. Tests: advertisement round-trip
  over a real loopback socket, malformed advertisement dropped,
  bounded dedupe, invite resolves a heard advertisement and refuses
  an unheard/ambiguous name, advertise switch default-off, discovery
  writes nothing to the registry.

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
  discover`/`peer invite` against others) must open that port itself.
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
- **`aoide peer invite <name>`** is sugar over the ceremony, nothing
  more: it runs its own discover sweep, resolves `<name>` to the
  matching advertisement (ambiguous or absent name = taught error
  listing what WAS heard), composes a dial target from that
  advertisement's OBSERVED source address on the house door port
  (`AOIDE_A2A_PORT` or 8710 — the wire carries no port to read; a far
  end on a non-default port takes the explicit `peer pair request
  <url>` path), shows the claimed hop and the observed address, and
  then runs the EXISTING `peer pair request` flow against the
  composed target — recording a `via` derived from the observed
  address plus the claimed login for the resulting peer's future
  calls. One ceremony stays the only verification.
- **`peer invite` refuses to invite yourself.** A broadcast always
  loops back to its own sender, so a box that advertises hears
  itself every sweep. Before dialing anything, invite checks whether
  the resolved target is this instance's own advertisement: the heard
  name matching this instance's own, or the datagram having come from
  loopback. Either one is a taught refusal, never a ceremony run
  against yourself. KNOWN GAP: a serve advertising under a custom
  `--peer-name` (flag only — the env/hostname tiers agree on both
  sides) escapes the name arm, and the self-heard broadcast arrives on
  the physical interface so the loopback arm misses too — `peer invite
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
- **`peer invite` records a `via` automatically; the ceremony itself does
  not require one.** Discovery's own observed source address (this
  document's Discovery section, above) is a real, reachable LAN target —
  the ceremony's own two POSTs still dial it directly by default, exactly
  as before this lane. What `peer invite` DOES do automatically is derive a
  `via` from that same observed address plus the advertisement's claimed
  ssh login and record it on the resulting peer once pairing is approved,
  so that peer's FUTURE calls (pull, spawn,
  send) have a working transport marker without a second manual step. An
  explicit `--via` on either `peer invite` or `peer pair request`
  overrides this for both the ceremony's own dial and the recorded marker.
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
