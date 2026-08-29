---
type: concept
created: 2026-08-28
updated: 2026-08-29
tags: [aoide, agent, a2a, orchestration, peer, security]
---

# Pairing Ceremony — one identity per instance, confirmed by both humans

Pairing is THE setup for connecting two aoide instances and the ONLY
verification between them: one ceremony commits a `pubkey`/`verified` peer
record on both ends and stamps the default permissions off the pair. Each
instance holds one ed25519 identity keypair (`aoide_storage::identity`,
minted lazily on first use through `load_or_mint`): two files under
`state/identity/` — `ed25519.key`, the raw 32-byte seed written once at
0600 inside the 0700 directory, never printed, logged, serialized, or put
on any wire; and `created_at` at 0644. `aoide identity [--json]` is the
read surface: it shows the full `pubkeyHex` and a `fingerprint` (the key's
first 8 bytes, colon-separated hex — a display label, distinct from the
two-sided SAS below). The daemon's session-sealing key is a SEPARATE
ephemeral keypair held in memory only, never these files ([[Session-Graph]]'s
identity section); the ceremony exchanges the two public keys and has both operators confirm
the same short code out of band. Design authority:
`docs/architecture/PAIRING.md`; wire shapes and constants: `CONTRACTS.md`
§6 ("Pairing wire", "Pairing events feed"); handlers
`pkgs/aoide/crates/server/src/a2a.rs` (`pair_request`/`pair_reveal`/
`pair_poll`), CLI half `pkgs/aoide/crates/client/src/
commands.rs` (`register_peer_pair`). The ceremony rides three methods on
the EXISTING [[A2A-Door]] — no new transport, no new port — and the
request/reveal pair is deliberately unauthenticated: the ceremony
establishes a credential where none exists yet, so gating it on one is
circular. The poll authenticates without a peer record — none exists on
the approver's side until the very id being polled is approved — so the
requester signs each poll with the same identity key whose pubkey rode
its request, and the approver verifies against the parked entry's own
stored pubkey.

## Commit-then-reveal — A commits, B does not

The handshake is commit-then-reveal (the Bluetooth SSP idiom). Only the
first mover (A, the requester) commits to its own nonce before revealing
it; B, the approver, reveals its own nonce immediately in its synchronous
response. The asymmetry closes the active-MITM gap a single round trip
leaves open: an on-path attacker who observes both real values in one
message controls enough of the SAS transcript to brute-force a 6-digit
code, while A's commitment fixes A's nonce before anyone learns it, so a
counterpart choosing its own values only after seeing B's response cannot
force a chosen code onto both operators.

```
box A (requester)                              box B (approver)
aoide peer pair <target> [--name b]
  (a URL dials directly; a bare hostname
   resolves it first, a ~45s discovery sweep)
  → aoide/pairRequest ───────────────────────► parks pending, UNREVEALED
    {pubkeyHex_A, name,                           (id, pubkey_A, name,
     commitHex = H(pubkey_A, nonce_A), url,        originAddr, url, commitHex,
     selfVia?}                                     selfVia if given)
  ◄── {id, pubkeyHex_B, nonceHex_B, expiresAt} ──┘  synchronous answer
  → aoide/pairReveal {id, nonceHex_A} ─────────► verifies H(pubkey_A,
                                                 nonce_A) == commitHex; on a
                                                 mismatch drops the entry
                                                 outright (-32002)
A's own invocation prints the derived SAS (NNN-NNN) and the request's short
id; both sides derive the SAME SAS locally, from values each already holds
operator B: peer pending → peer pair approve [<id>]
  B's operator TYPES the code as read off A's screen (out-of-band;
  --code NNN-NNN scripted), compared against B's OWN derived SAS,
  never echoed in the prompt; a wrong code counts a persisted try,
  the 3rd cumulative mismatch auto-denies (nothing committed)
  On a match, B's own peer record commits HERE — purely     (nothing
  local, and the parked entry is marked approved, left PARKED  dials A)
operator A: a SECOND peer pair approve <id> (against the outbound queue)
  → aoide/pairPoll {id, timestampIso, nonceHex, ──────────► verified against
    signatureHex} — over the SAME forward dial                 the parked entry's
    the request/reveal used                                own pubkeyHex_A;
  ◄── {status: approved, pubkeyHex_B} ─────────────────────┘ else the identical
                                                              {status: pending}
  A re-derives the SAS, its operator confirms y/N, and A's own peer
  record commits. peer pair reject <id> aborts either queue at any
  stage — no wire call, no record on either end.
```

Nothing ever dials IN to the requester: the poll rides the same forward
dial (or tunnel) the request already used, so a requester whose own door
binds loopback-only completes the ceremony end to end. The poll's
existence-oracle discipline is byte-uniform: an unknown id, a bad
signature, and a not-yet-approved entry all answer `{status: pending}`;
only a poll that verifies AND finds the entry approved releases B's
pubkey, which A checks against the value B's synchronous
`aoide/pairRequest` answer already gave it before marking its outbound
entry `awaiting-confirm`.

## The commit asymmetry — both humans confirm

The two ends commit their peer records asymmetrically, and the two
confirmations differ by side. B's record for A exists the moment B's
operator approves — `peer pair approve` on an inbound id is purely local
(`aoide-client::commands::approve_inbound`): it commits the record and
marks the parked entry approved, dialing nobody. B's gate is the TYPED
code: the operator types the SAS as read off A's screen, out of band, and
it is compared against B's own locally derived SAS — the prompt never
echoes the expected value. A wrong code counts one try, persisted on the
parked entry across invocations; the third cumulative mismatch
auto-denies (the same clean removal `reject` performs, audited
`auto-deny-on-code-mismatch`), and an entry already at the try limit is
denied on sight. A's record for B exists only once A's own operator runs
`peer pair approve <id>` a second time, against the OUTBOUND queue — that
invocation polls `aoide/pairPoll` first, then re-derives the SAS from
values already held locally and asks for the y/N confirm (A's own screen
printed the code at request time, so the typed-code gate is B's side
only; `--yes` scripts A's confirm but never bypasses B's). A code shown
once and accepted once is not a mutual confirmation; a never-confirmed A
leaves B holding a `verified: true` peer that answers nothing — visible
on B's own `peer status`, resolved by an ordinary expiring re-pair.

## The SAS

The short authentication string is `aoide_storage::pairing::derive_sas`:
SHA-256 over the four public transcript values (requester pubkey,
approver pubkey, requester nonce, approver nonce), each lowercased,
trimmed, and NUL-separated, then the first four digest bytes as a
big-endian u32 mod 1,000,000, formatted `%03d-%03d`. Each side derives it
locally from values it already holds — the SAS never rides the wire, and
neither side trusts a wire-carried code. The transcript is
order-sensitive, not a set; `CONTRACTS.md` pins the stability vectors
(`derive_sas_stability_vectors_never_drift`):
`derive_sas("a"*64, "b"*64, "c"*16, "d"*16) == "740-729"`, and swapping
the requester/approver roles on the same four values yields `"847-405"`.

## The command surface

`aoide peer pair <target> [--name <n>] [--via ssh://[user@]host[:port]]
[--self-via ssh://[user@]host]` / `aoide peer pending` / `aoide peer pair
approve [<id>] [--yes] [--code NNN-NNN]` / `reject <id>` / `watch [--popup]
[--json]`, registered together in `register_peer_pair`:

- **`peer pair <target>`** takes exactly ONE positional and runs both arms
  of the request through the same `run_pair_request` core: a URL-shaped
  target (`"://"`) dials directly; anything else is a hostname resolved by
  the command's own ~45s discovery sweep (ambiguous or absent name = taught
  error listing what WAS heard). A second positional is refused outright,
  before either arm runs, naming the fold in the refusal; the `approve`/
  `reject`/`watch` subcommand names match BEFORE a bare hostname positional
  of the same spelling, so a box actually named `approve` pairs only by the
  explicit URL form. It sends the commitment and the reveal as two
  sequential POSTs inside one invocation, parks the outbound half
  (`state/peer-pairing-outbound.json`), and prints the derived SAS plus the
  request's short id.
- **`peer pending`** lists BOTH directions parked on this instance, each row
  carrying the host, id, direction, and state (`awaiting-approval` /
  `awaiting-confirm`; an unrevealed inbound entry shows `revealed: false`
  and `approve` refuses it with a taught "awaiting reveal" error, since the
  transcript needs A's nonce) — and NEVER the SAS (P-PV2): the code stays
  out-of-band, read off the requester's screen and typed on the
  approver's, never printable in a listing either operator can glance at.
- **`approve [<id>]`** dispatches by direction — the inbound queue first,
  then the outbound — and re-derives the SAS from this instance's own
  identity either way; the id is optional when exactly one request is
  pending. The gates differ by side (the commit asymmetry
  above): inbound asks the operator to TYPE the code (`--code NNN-NNN`
  scripted; `--yes` never bypasses), outbound polls and then asks `y`/`N`
  (`--yes` scripted). Nothing commits before the side's own gate passes.
- **`reject <id>`** is a clean local refusal on either queue — no wire
  call, no peer record; on an outbound entry it doubles as the ceremony's
  abort command, usable at any stage.
- **`watch`** is the foreground follow of the events feed (below):
  narrates each recognized line (`--json` emits the event object verbatim
  instead) and re-derives the actionable set from
  `aoide_storage::pairing::list_inbound`/`list_outbound` on a 30s
  reconcile tick, so a missed or malformed line never strands a request.
  The actionable set is an inbound entry once revealed; an outbound entry
  reaches `awaiting-confirm` only synchronously inside the operator's own
  `peer pair approve`, so watch never surfaces an outbound completion on
  its own — polling and confirming an outbound request is always the
  operator's own invocation.
  `--popup` swaps the narration for a dialog shaped by DIRECTION — never
  one bare yes/no for both. `lyra pair ask`/`lyra pair confirm` (sharing
  `lyra secrets ask`'s own quickshell surface via `dialog_qml`) when
  `lyra` resolves, `zenity --entry`/`zenity --question` otherwise, falling
  back to zenity on a `lyra` failure for that one attempt (refused up
  front only when NEITHER binary resolves; `--popup`+`--json` is a usage
  error). On an inbound (approver) request the dialog COLLECTS a typed
  code and runs it through `InboundGate::Code` — the identical SAS
  comparison and three-try auto-deny the CLI's `--code`/tty prompt use,
  and the dialog NEVER shows the code, matching the tty prompt's own
  "never echo the SAS" rule. On an outbound (requester) request the dialog
  SHOWS this instance's own locally-derived code (not a leak — the CLI's
  own `y`/`N` confirm already prints it) and asks for a single
  Approve/Reject, never a retype — `approve_outbound` commits
  unconditionally on Approve, exactly as it always has (a design-review
  round within this same phase caught an earlier pass collecting a retype
  on this arm too as copy-the-pixels theater, since the code was already
  on screen in the same window). Both directions' exit-0 Approve and
  `"Reject request"` extra button/dismiss control drive the SAME
  `approve_inbound`/`approve_outbound` paths the CLI runs: the CLI is
  always sufficient, and the popup is a second door over the same
  primitive. A graphical session runs it as the `aoide-pair-watch.service`
  user unit (`modules/nucleus/aoided.nix`), gated on `aoide.a2a.enable &&
  aoide.facets.quickshell.enable && aoide.pairing.popup` — the last of
  those OFF by default; a host with a2a and the quickshell facet on does
  not get the popup unless it also opts in.

## The events surface

From the Ok arm of `aoide/pairRequest` and `aoide/pairReveal` — never from
a mismatch or unknown-id arm — `a2a serve` appends one record to `aoided`'s own
events feed (`emit_pairing_event` in `aoide-server::a2a`; best-effort,
never `?`, never panicking): `$XDG_RUNTIME_DIR/aoide/events.jsonl`
(`aoide_server::daemon::events_path`), written through its own
`aoide_protocol::feed::FeedWriter` onto the same path the daemon writes.
Each record is `class: "gate"`, `source: "a2a-door"`, with `kind`
`pair-parked` (`pairRequest`) or `pair-revealed` (`pairReveal`).
`aoide/pairPoll` emits nothing: a poll answered is no state change on the
approver's side worth surfacing. A third kind, `pair-awaiting-confirm`, is
defined but has no emitter, so the watcher's outbound half never fires on
its own. The payload carries
`id`/`name`/`originAddr`/`url`/`direction` BY NAME ONLY — never a SAS,
pubkey, nonce, or commitment. The feed line is a trigger, never trusted
data: `peer pending`'s storage-backed list is the authority, and the
watcher's reconcile tick re-derives the truth from
`aoide_storage::pairing` directly, the same stance the secrets broker's
own events feed holds. Two writers (`a2a serve` and `aoided`) share the
one file, capped at `EVENTS_CAP_BYTES` (1 MiB) and truncated in place — a
cap-truncate race can lose a line, accepted because the durable record is
the single audit log, written at every call site regardless.

## Park cap and expiry

The inbound park queue is capped (`aoide_storage::pairing::park_inbound`):
beyond `AOIDE_PAIRING_PARK_CAP` concurrently parked requests — default 32
(`DEFAULT_PAIRING_PARK_CAP`) — parking refuses with `-32000`, a taught
error naming the cap and its override, checked under one lock immediately
before insert. Outbound entries are operator-created (one per `request`
invocation, never wire-driven) and carry no cap. Parked entries expire
after `AOIDE_PAIRING_TIMEOUT` seconds, else the 4-hour default
(`DEFAULT_PAIRING_TIMEOUT_SECS`) — the wait is for a human, twice; expired
entries are swept lazily on the next `list`/`take` call, never by a
background timer. Nothing about parking ever reaches `state/peers.json`
until an explicit approve.

## What approval commits

A fully approved request is the ceremony's entire grant:
`peer_store::upsert_paired_peer` commits a peer record carrying
`pubkey`, `verified: true`, and the default `allows: ["read","spawn"]`
(the closed capability vocabulary `PEER_CAPABILITIES`). When the parked
request carries the requester's optional `selfVia` claim (its
self-asserted `ssh://[user@]host` reach-back hop — self-asserted data, a
transport marker only, never a source of trust), the approver's commit
also sets the resulting peer's `url` to `http://127.0.0.1:<port>/` —
loopback-as-seen-from-the-far-side, `<port>` parsed off the REQUESTER's
own advertised url — and `via` to the claim, in the same write; absent
the claim, `url` is the advertised door verbatim and `via` stays unset
([[Peer-Transport]]). Narrowing or
widening that grant afterward is `peer allow <name> <cap> on|off`'s job —
idempotent, refusing unknown capability strings — never re-run by
re-pairing. The upsert matches by NAME, so one remote instance paired
under two names yields two records sharing one key — harmless, because
identity IS the key: inbound signed requests resolve by whichever verified
record's stored pubkey verifies the signature, the claimed name demoted to
attribution plus an exact-name tiebreak among same-key records
([[Peer-Federation]]'s Signature rung). The record is what the [[A2A-Door]]'s gates read: Spawn
admits only a caller resolved through the Signature rung whose `allows`
contains `"spawn"` (`a2a.rs::spawn_admitted`), and a signed peer's
per-request ed25519 signature is the wire authentication the ceremony's
verified key anchors. See
[[Peer-Federation#Security — pairing is the verification path]].

## Legacy escapes

The door-wide bearer (`aoide.a2a.tokenFile`/`aoide.a2a.bearerSecret`) and
a peer's own `tokenFile` are legacy escapes for an UNPAIRED caller: they
authenticate the read arms (`tasks/get`, the AgentCard GET,
`aoide/graphSummary`) and answer Inject's autogate question, and nothing
else — the door-wide bearer alone never resolves a peer identity, and
neither rung reaches Spawn. A bare address match (`PeerRung::Addr`)
resolves a peer identity for attribution only — Inject's `from` field,
the autogate question — since it carries no possession proof. `peer pair`
authenticates a peer without any pre-shared secret. Spec:
`CONTRACTS.md` §6 "Legacy escapes".

## Status

Real: the three wire methods (`aoide/pairRequest`/`aoide/pairReveal`/
`aoide/pairPoll`), both parked-state files, the collapsed CLI surface
(`peer pair <target>`, `peer pending`, `pair approve`/`reject`/`watch`),
the SAS derivation with its pinned vectors, the typed-code gate with its
persisted tries and auto-deny, the events feed, and the popup (typed-code
entry via `lyra
pair ask` or zenity `--entry`, opt-in behind `aoide.pairing.popup`) all
run.

## Related

- [[A2A-Door]] — the door the ceremony's three methods ride, and the
  gates that read the record it commits
- [[Peer-Federation]] — the registry the verified peer lands in
- [[Peer-Transport]] — `--via` on `peer pair` dials through a
  tunnel and records the marker at approve
- [[aoide-cli]] — the registry the pairing commands register into
- `docs/architecture/PAIRING.md` — the design authority for the ceremony
