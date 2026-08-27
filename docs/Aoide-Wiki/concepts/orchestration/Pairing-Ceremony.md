---
type: concept
created: 2026-08-28
updated: 2026-08-28
tags: [aoide, agent, a2a, orchestration, peer, security]
---

# Pairing Ceremony — one identity per instance, confirmed by both humans

Pairing is THE setup for connecting two aoide instances and the ONLY
verification between them: one ceremony commits a `pubkey`/`verified` peer
record on both ends and stamps the default permissions off the pair. Each
instance holds one ed25519 identity keypair (`aoide_storage::identity`,
minted lazily on first use, the private key at 0600 and never printed);
the ceremony exchanges the two public keys and has both operators confirm
the same short code out of band. Design authority:
`docs/architecture/PAIRING.md`; wire shapes and constants: `CONTRACTS.md`
§6 ("Pairing wire", "Pairing events feed"); handlers
`pkgs/aoide/crates/server/src/a2a.rs` (`pair_request`/`pair_reveal`/
`pair_approve_callback`), CLI half `pkgs/aoide/crates/client/src/
commands.rs` (`register_peer_pair`). The ceremony rides three methods on
the EXISTING [[A2A-Door]] — no new transport, no new port — and all three
are deliberately unauthenticated: the ceremony establishes a credential
where none exists yet, so gating it on one is circular.

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
aoide peer pair request <url> [--name b]
  → aoide/pairRequest ───────────────────────► parks pending, UNREVEALED
    {pubkeyHex_A, name,                           (id, pubkey_A, name,
     commitHex = H(pubkey_A, nonce_A), url}        originAddr, url, commitHex)
  ◄── {id, pubkeyHex_B, nonceHex_B, expiresAt} ──┘  synchronous answer
  → aoide/pairReveal {id, nonceHex_A} ─────────► verifies H(pubkey_A,
                                                 nonce_A) == commitHex; on a
                                                 mismatch drops the entry
                                                 outright (-32002)
both sides derive the SAME SAS locally, from values each already holds
operator B: peer pair pending → peer pair approve <id>
  ── aoide/pairApprove {id, pubkeyHex_B} ─────►► A marks its outbound entry
  B's own peer record commits HERE                 awaiting-confirm (no record)
operator A: peer pair pending shows the SAME SAS a second time;
a SECOND peer pair approve <id> (against the outbound queue) commits
A's own peer record. peer pair reject <id> aborts either queue at any
stage — no wire call, no record on either end.
```

## The commit asymmetry — both humans confirm

The two ends commit their peer records asymmetrically. B's record for A
exists the moment B's operator approves — `peer pair approve` delivers the
`aoide/pairApprove` callback BEFORE writing anything local
(`aoide-client::commands::approve_inbound`). A's record for B exists only
once A's own operator runs `peer pair approve <id>` a second time, against
the OUTBOUND queue, re-deriving the SAS from values already held locally
with no further wire call. A code shown once and accepted once is not a
mutual confirmation; a never-confirmed A leaves B holding a `verified:
true` peer that answers nothing — visible on B's own `peer status`,
resolved by an ordinary expiring re-pair.

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

## The five commands

`aoide peer pair request <url> [--name <n>] [--self-url <url>] [--via
ssh://[user@]host[:port]]` / `pending` / `approve <id> [--yes]` /
`reject <id>` / `watch [--popup] [--json]`, registered together in
`register_peer_pair`:

- **`request`** sends the commitment and the reveal as two sequential
  POSTs inside one invocation, parks the outbound half
  (`state/peer-pairing-outbound.json`), and prints the derived SAS.
- **`pending`** lists BOTH directions parked on this instance: inbound
  requests (an unrevealed entry shows `revealed: false` and no SAS —
  `approve` refuses it with a taught "awaiting reveal" error, since the
  transcript needs A's nonce) and outbound ones, tagged with their state
  (`awaiting-approval` / `awaiting-confirm`).
- **`approve <id>`** dispatches by direction — the inbound queue first,
  then the outbound — re-derives the SAS from this instance's own
  identity either way, and requires an explicit `y`/`yes` (`--yes` for
  scripted use) before anything commits.
- **`reject <id>`** is a clean local refusal on either queue — no wire
  call, no peer record; on an outbound entry it doubles as the ceremony's
  abort command, usable at any stage.
- **`watch`** is the foreground follow of the events feed (below):
  narrates each recognized line (`--json` emits the event object verbatim
  instead) and re-derives the actionable set — an inbound entry once
  revealed, an outbound entry once `awaiting-confirm` — from
  `aoide_storage::pairing::list_inbound`/`list_outbound` on a 30s
  reconcile tick, so a missed or malformed line never strands a request.
  `--popup` swaps the narration for a zenity `--question` confirm dialog
  per actionable request (refused up front when zenity is not on PATH;
  `--popup`+`--json` is a usage error). The dialog's exit-0 Approve and
  its `"Reject request"` extra button drive the SAME
  `approve_inbound`/`approve_outbound` paths the CLI runs (`skip_confirm:
  true` — the dialog itself IS the confirmation): the CLI is always
  sufficient, and the popup is a second door over the same primitive. A
  graphical session runs it as the `aoide-pair-watch.service` user unit
  (`modules/nucleus/aoided.nix`), gated on `aoide.a2a.enable &&
  aoide.facets.quickshell.enable`.

## The events surface

From the Ok arm of each of the three wire methods — never from a mismatch
or unknown-id arm — `a2a serve` appends one record to `aoided`'s own
events feed (`emit_pairing_event` in `aoide-server::a2a`; best-effort,
never `?`, never panicking): `$XDG_RUNTIME_DIR/aoide/events.jsonl`
(`aoide_server::daemon::events_path`), written through its own
`aoide_protocol::feed::FeedWriter` onto the same path the daemon writes.
Each record is `class: "gate"`, `source: "a2a-door"`, with `kind`
`pair-parked` (`pairRequest`), `pair-revealed` (`pairReveal`), or
`pair-awaiting-confirm` (`pairApprove`). The payload carries
`id`/`name`/`originAddr`/`url`/`direction` BY NAME ONLY — never a SAS,
pubkey, nonce, or commitment. The feed line is a trigger, never trusted
data: `peer pair pending`'s storage-backed list is the authority, and the
watcher's reconcile tick re-derives the truth from
`aoide_storage::pairing` directly, the same stance the secrets broker's
own events feed holds. Two writers (`a2a serve` and `aoided`) share the
one file, capped at `EVENTS_CAP_BYTES` (1 MiB) and truncated in place — a
cap-truncate race can lose a line, accepted because the durable record is
the single audit log, written at all three call sites regardless.

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
(the closed capability vocabulary `PEER_CAPABILITIES`). Narrowing or
widening that grant afterward is `peer allow <name> <cap> on|off`'s job —
idempotent, refusing unknown capability strings — never re-run by
re-pairing. The record is what the [[A2A-Door]]'s gates read: Spawn
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
the autogate question — since it carries no possession proof. `peer pair
request` is the replacement for the hand-wired shared secrets. Spec:
`CONTRACTS.md` §6 "Legacy escapes".

## Status

Real: the three wire methods, both parked-state files, the five CLI
commands, the SAS derivation with its pinned vectors, the events feed,
and the zenity popup all run. `--popup` is zenity `--question` only; a
QML confirm dialog is a named deferral, not built.

## Related

- [[A2A-Door]] — the door the ceremony's three methods ride, and the
  gates that read the record it commits
- [[Peer-Federation]] — the registry the verified peer lands in
- [[Peer-Transport]] — `--via` on `peer pair request` dials through a
  tunnel and records the marker at approve
- [[aoide-cli]] — the registry the five commands register into
- `docs/architecture/PAIRING.md` — the design authority for the ceremony
