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
   `"read"` (graph/who summaries over A2A). Verbs:
   `aoide peer allow <name> <cap> on|off` — idempotent, reports
   exactly what changed. Unknown capability strings are refused.
   `autogate` stays its own field (landed semantics, not churned).
   Default for a PAIRED (verified) peer: `["read", "spawn"]`.
   An unpaired peer: empty set. Session-level granularity is #63's
   lane, not this one.
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
  `aoide identity` verb — the executor proposes the verb shape),
  stored under the state dir, private key 0600, never printed, never
  in an Outcome. The public key and its fingerprint are freely
  shown.
- The broker is not the keystore for the identity (the daemon/CLI
  must read it without a TOTP ceremony); the identity file follows
  the storage crate's private-file discipline
  (`atomic_write_private` precedent).

## The ceremony

```
box A                                      box B
aoide peer pair request <url> [--name b]
  → POST pairing request to B's door ────► parks pending (id, A's pubkey,
     (A's pubkey, A's name, nonce)          A's claimed name, origin addr)
                                           operator: CLI prompt on next
                                           `aoide peer pair pending` /
                                           popup via the events feed
both sides display: SAS code derived from (pubkey_A, pubkey_B, nonces)
human confirms codes match (CLI y/N or popup confirm)
                                           aoide peer pair approve <id>
  ◄── B's pubkey + approval ──────────────┘
peer records written BOTH ends: pubkey, verified=true,
allows=["read","spawn"], names bound
```

- The short authentication string (SAS) is derived from a transcript
  hash over both public keys + both nonces (executor states the exact
  derivation; standard SAS construction, no invention).
- A pairing request that is never approved expires (timeout knob,
  default generous — hours, not minutes; it waits for a human).
- Re-pairing an existing peer replaces the key material only after
  the same confirmation — never silently.

## Wire authentication (paired peers)

Per-request detached signature replaces bearer comparison for paired
peers:

- Headers: peer name, timestamp, nonce, signature. Signature is
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

## Phases

Serialized, Sonnet exec + different-Sonnet review each, cargo field
exclusive per phase. Golden count changes ride the same commit with
the full count-site checklist (git show 9c2d05c).

- **P-P1 — identity + the dependency (M).** The vetted dep lands
  (workspace-level justification comment in the Cargo.toml), keypair
  mint/load/store in `storage` (or a new small crate if storage's
  charter resists — executor states the reading), `aoide identity`
  show verb (golden +1). Tests: mint-once idempotence, 0600, pubkey
  round-trip, no private material in any Serialize type (grep gate).
- **P-P2 — pairing wire + CLI ceremony (L).** Request/park/approve
  over the A2A door (new method, CONTRACTS §6), SAS derivation +
  display, `peer pair request|pending|approve|reject` verbs (golden
  +N), peer record gains `pubkey`/`verified` (additive serde),
  legacy fields untouched. Tests: full ceremony against a test door,
  SAS stability vectors, expiry, re-pair confirmation, unapproved =
  no record change.
- **P-P3 — allows + the spawn gate flip + audit identity (M).**
  `allows` set + `peer allow` verbs (golden +1), spawn arm requires
  paired+spawn (taught error otherwise), audit/pending/session-record
  stamping (`origin: peer:<name>`, projected into the ledger).
  Tests: gate table (paired+allowed / paired+denied / unpaired /
  legacy-bearer), stamp presence end-to-end, defaults per decision 5.
- **P-P4 — signed requests (L).** The wire-auth scheme above for
  paired peers; unpaired callers unchanged. Tests: signature
  round-trip, replay rejection, skew taught error, canonical-string
  vectors pinned.
- **P-P5 — popup + polish (S).** The pairing-request popup surface
  riding the events feed (same seam as secrets asks), legacy-escape
  documentation pass (CONTRACTS + wiki page for the ceremony).

Live gates at the end of the lane: a real pair between yomi and
sakaki via the ceremony (codes compared on real terminals), a spawn
refused for a peer with spawn revoked, a spawn admitted and its
ledger entry carrying `origin: peer:<name>`.

## Kill-list

- No hand-rolled cryptography — primitives come from the vetted dep,
  full stop (User ruling).
- No daemon-door or A2A allowlist tables — the per-verb door policy
  and the `allows` set are the only gates (AOIDED L2 discipline).
- No transitive relay, no mesh-level object — the mesh stays the
  closure of pairwise records.
- No per-capability serde bool scatter — `allows` is one field.
- The identity private key never crosses any socket, any Outcome,
  any log — including the broker's.
