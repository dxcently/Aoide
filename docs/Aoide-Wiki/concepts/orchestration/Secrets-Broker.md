---
type: concept
created: 2026-08-23
updated: 2026-08-28
tags: [aoide, agent, cli, secrets, totp, security, broker]
---

# Secrets Broker — the `aoide secrets` door

The secrets broker is aoide's credential door: a socket-only daemon, running
under its own system uid, that resolves a named secret to a value for an
authorized consumer and never lets that value touch a log, an audit line, or
a cached copy. It is a fifth door onto aoide alongside the CLI trunk, MCP,
[[A2A-Door]], and [[Node-Federation]] — reachable only from the local
machine, never the network. Implementation: `pkgs/aoide/crates/secrets/`.
Wire contract: `CONTRACTS.md`'s "Secrets wire" subsection.

## Identity and the release-to-client flow

The broker runs as its own system user, `aoide-secrets` — never the
operator's uid, never root. Its secrets home (`/var/lib/aoide-secrets`,
mode `0700`) holds `policy.json`, the TOTP secret, the replay ledger, and
the backend-store files; its unix socket
(`/run/aoide-secrets/secrets.sock`, mode `0660`, group
`aoide-secrets-access`) is the ONLY door onto any of it. Membership in
`aoide-secrets-access` is what lets a local session reach the socket at
all.

The broker never execs a caller's command — it would inherit the wrong
cwd/env and hand secrets privileges to an arbitrary child. Instead, a value
releases across the socket into the CALLING process:

```
agent  ──►  aoide secrets exec --as <consumer> --secret <name> [--totp NNNNNN] -- <cmd>
        ──►  client (caller's own uid) connects, sends {op:"resolve", secret, consumer, totp?}
        ──►  broker (aoide-secrets uid): policy gate (exists → consumer
             authorized → remote-origin) → TOTP gate (if required)
             → fetch via the backend, AS THE BROKER'S OWN UID
             → release the value over the socket
        ──►  client injects it as an env var and execs the wrapped command,
             returning that child's own exit code
```

The value exists only inside the client process's own environment, from the
moment it reads the socket reply to the moment it execs — never in argv,
never in an audit line, never cached anywhere. Both audit lines (the
broker's own `audit.log` and the mirrored aoide audit log) are written
BROKER-SIDE, before the value is ever released.

`aoide secrets put <name>` is the write-side mirror, reading a value from
stdin (a terminal prompts with echo hidden; a pipe reads straight through)
and sending it to the broker over the same socket, never through argv. A
`put` against a secret that already has a stored value is refused with a
machine-readable `exists` flag unless the caller passes `--force` — on a
terminal this becomes a `y/N` confirmation instead of a silent overwrite.
`put` carries no `consumer` field and is never TOTP-gated: it is a CLI-only,
operator-side command (run as `sudo -u aoide-secrets aoide secrets put …` in
deployment), not an agent-facing one.

## Policy: consumers, TOTP, automation, remote reachability, and caller origin

Every secret is exactly one policy entry: a name, a backend, a key, a
`consumers[]` list, a `requireTotp` bit, an `automation` gate, a `remote`
bit, and an `allowRemoteOrigin` bit. A credential with multiple fields (a
username and a password) is modeled as multiple named secrets, each with its
own policy — there is no multi-field resolve.

`consumers[]` is checked against the wire's `consumer` field, which is
**self-asserted** — nothing on the socket authenticates it. The group
membership required to reach the socket at all is the real boundary;
`consumers[]` is a courtesy label on top of it, not a cryptographic one.
The sealed session credential authenticates the calling SESSION and its
origin class, never this string — consumer-name authentication is a
separate, unbuilt axis (the identity lane's first named remainder,
[[Session-Graph]]'s accounting).

**TOTP.** A secret with `requireTotp: true` verifies a caller-supplied code
against a per-host RFC 6238 enrollment (`aoide secrets enroll`, hand-rolled
SHA-1/HMAC/TOTP/base32 — zero algorithmic dependencies) with a `±1` timestep
window. A matched code is consumed once in a persisted replay ledger keyed
on TIMESTEP alone — never on consumer, since the consumer field can't be
trusted to distinguish callers. `requireTotp` is unresolvable only when no
enrollment exists on the host at all; once enrolled, a missing, wrong, or
already-used code is an ordinary denial, never a silent standing grant.
`secrets enroll --show` reprints the current enrollment's URI/base32/QR
without rotating anything; `secrets enroll --force` rotates the secret and
resets the replay ledger together.

**Automation.** A policy's `automation: {enabled, consumers[]}` names
specific consumers that resolve a `requireTotp`-gated secret without a fresh
code, while every other caller stays gated exactly as before. Automation
can only ever RELAX `requireTotp`, never impose it on a policy that doesn't
already carry it. `secrets automate <name> on|off` and `secrets automate
<name> grant|revoke <consumer>` edit this field; it is checked against the
same self-asserted `consumer` field every other gate trusts, so an
automation-open secret is effectively code-free for any local socket caller
claiming a listed name.

**Remote reachability.** A policy's `remote` bit (default `false`) gates
nothing today — no non-local entry point onto the broker exists. It exists
so an operator can pre-declare which secrets are meant to ever leave the
host; every non-local entry point added later refuses a secret whose
`remote` is `false` before touching its backend. `secrets expose <name>
on|off` flips it.

**Caller origin.** A policy's `allowRemoteOrigin` bit (default `false`;
`secrets allow-remote-origin <name> on|off` flips it) is the third axis and
the sealed session credential's first policy consumer ([[Session-Graph]]).
The three axes answer three different questions and are never conflated:
`remote` is transport — may this secret be SERVED through a non-local entry
point; `automation` is the code — may a listed consumer skip TOTP;
`allowRemoteOrigin` is caller provenance — may a session a REMOTE NODE
created resolve this secret LOCALLY. Gate order is exists → consumer
authorized → remote-origin → TOTP → fetch. The broker resolves each
connection's caller from kernel facts alone — the connection's
`SO_PEERCRED` pid, a walk of its real `/proc` ancestry to a sealed session
record, and the seal verified against the daemon's live `ping`-fetched
public key (`aoide_storage::attest::attested_caller`, the same walk and
verify the send gate uses; nothing the wire asserts enters it) — and
refuses a caller whose sealed `originClass` is `node:*` before the
TOTP/park branch unless the bit is on. The refusal names the flag, the
session, and its origin, and audits name-only. The boundary is exact: the
gate narrows positively-attested remote-origin sessions; it does not
authenticate local ones. An UNIDENTIFIED caller — no sealed session in its
ancestry, an unreachable daemon, an unreadable roster — is not refused:
local unidentified callers were always admitted, and the gate keys only on
positive attestation (the lane's same-uid honesty, OQ1-A). The same
residual composes through the daemon's reseal sweep: a same-uid process
can append an unsealed roster row asserting `origin: local` for its own
pid, and the sweep signs whatever the row asserts — laundering a forged
local class into a POSITIVE attestation, closable only by an authenticated
registration path (a named remainder). In the packaged cross-uid deployment
(the broker runs as the `aoide-secrets` system user) the operator's daemon
socket and session roster are unreachable from the broker, so every caller
there resolves unidentified and the gate is dormant; a cross-uid
attestation channel is a named, deliberately unbuilt remainder, and the
gate bites wherever the broker runs as the operator's own uid (the
cargo-only deployment).

## Parked asks — a codeless TOTP resolve waits instead of refusing

A `requireTotp` resolve with no code PARKS rather than refusing outright:
the requesting connection registers `{id, secret, consumer, requestedAt}`
in the broker's in-memory registry and blocks, open, until a SEPARATE
connection completes or dismisses the ask, or a timeout elapses (default
300 seconds, `AOIDE_SECRETS_PARK_TIMEOUT` to change it). The broker
announces the park to the parked connection itself as an interim wire line
before it ever blocks, so an interactive caller learns its own ask id
immediately rather than watching a silent hang.

```
agent     ──► aoide secrets exec --as m --secret db-prod -- psql   (no --totp)
          ──► parks: {id: "3f2a-3", secret: "db-prod", consumer: "m", …}

operator  ──► aoide secrets pending                       (a separate connection)
          ──► aoide secrets approve 3f2a-3 --totp 123456   (a third connection)
              → verifies the code, RE-RUNS the full authorization gate
                against the ask's STORED consumer, fetches fresh, and
                releases the value down the ORIGINAL connection

          ──► aoide secrets dismiss 3f2a-3      (or: let it time out)
```

`approve` re-running the authorization gate at release time (not merely
trusting the stored ask) is what keeps a `secrets revoke` issued while an
ask sits parked effective: a revoked consumer's parked ask denies at
release, not merely at the moment it first parked. A wrong code leaves the
ask parked, ledger unburned; a right code that the re-gate then denies
returns the same denial to both the operator's `approve` call and the
agent's original `resolve` call, and the ask is removed either way. `wait:
false` on the wire (no CLI flag) restores the pre-parking immediate refusal
for a caller with no way to ever supply a code. A registry-wide cap (default
32, `AOIDE_SECRETS_PARK_CAP`) bounds how many asks may sit parked at once;
beyond it, a codeless resolve gets the immediate refusal instead of a park.

## `secrets watch` — the terminal completion surface

`aoide secrets watch` is a foreground, line-mode surface that tails a
broker-owned events feed (a JSON-lines file living beside the broker's own
socket, capped at 1 MiB and truncated in place past the cap) and narrates
five event kinds: `released` (a TOTP-free grant), `parked`,
`completed` (a successful `approve`), `dismissed`, and `expired` (a park that
timed out). On
a terminal, a parked ask prompts inline — `[a]` approve with a hidden code
prompt, `[d]` dismiss, `[i]` ignore for this session only (the ask stays
parked, completable from elsewhere) — FIFO by whichever ask is closest to
expiry. `--json` emits one event object per line, flushed per line, the
pickup point a graphical consumer subscribes to instead of re-tailing the
feed itself.

The events feed is a push signal, not the authority: `secrets pending`'s
own poll of the broker's in-memory registry is, and `watch` reconciles
against it once at startup and again on every event plus a 30-second safety
tick, so a missed line, a truncation, or a broker restart never leaves a
parked ask permanently invisible. Two near-expiry lockouts protect a code
from being typed and spent too close to the deadline: a code prompt refuses
to open below 10 seconds remaining, and an already-open prompt is force-
closed once an ask crosses below 15 seconds, without reopening — the ask
itself stays parked, completable from elsewhere, the whole time.

**`--popup`** swaps the terminal prompt for a `zenity --entry --hide-text`
dialog with an added "Dismiss ask" button — Cancel maps to ignore, the
extra button maps to dismiss, and the two are always visually and texturally
distinct so neither is mistaken for the other. A dialog opens only when the
screen is unlocked, checked by ORing `loginctl`'s `LockedHint` with a
`/proc` scan for a named locker process (`AOIDE_SECRETS_LOCKER`, default
`hyprlock`) — the process scan exists because some lockers set no
`LockedHint` at all. A failing `zenity` spawn backs off from 1 second up to
a 60-second ceiling globally across every ask, rather than retrying on
every poll tick forever. `released`/`completed`/`dismissed`/`expired` never
drive a dialog — only `parked` does — so a busy automation consumer never
produces a storm of popups, only scrolling terminal narration.

## The age lane — the built-in encrypted backend

`aoide`'s own two stores, `file` and `age`, are the only backend
IMPLEMENTATIONS this crate supports; `pass`/`gopass`/`bw`/`sops` are
documentation-only presets — a shape to copy into `backends.json` by hand,
with no integration support. `age` is `secrets add`'s DEFAULT backend when
`--backend` is omitted: each secret is an age-encrypted `0600` file, and the
decrypting identity is minted lazily, on the broker's own first `age`-backed
write — never on a read, which is a taught error instead of a silent mint if
no identity exists yet. `file` (plain `0600` files) is the explicit
fallback. Both backends are expressed entirely through the same
shell-template mechanism every backend uses — no code path is special-cased
to either one's bytes.

`aoide secrets migrate <name> [--backend <target>]` (target defaults to
`age`) moves one secret's value from its current backend to a target
backend: fetch → store on the target (durable) → flip and save the policy →
remove the old value LAST, strictly after the policy flip has already
succeeded, so a failure at any earlier step leaves the secret fully
resolvable on its original backend. Old-value removal only ever happens for
a built-in source backend whose on-disk path this crate can derive on its
own — a `pass`/`gopass`/`bw`/`sops` or hand-customized source is left
untouched, and the migration reports that plainly.

Every backend shell-out (`get`/`set`/`has`) runs through one bounded spawn
path — `AOIDE_SECRETS_BACKEND_TIMEOUT` (default 10 seconds) bounds the
wait, and a template still running past the deadline has its whole process
group killed and reaped, never left as an orphan or a zombie. A hung `get`
blocks only its own connection's thread; every other connection is served
normally the entire time.

## Deployment

The nix module (`modules/nucleus/secrets.nix`, `aoide.secrets.enable`,
default off) provisions the `aoide-secrets` system user, the
`aoide-secrets`/`aoide-secrets-access` groups, and a system service
(`aoide-secrets-serve`, anchored to `multi-user.target`, no graphical-session
dependency — the broker has no business caring whether a desktop session
exists) with the socket path and secrets home set explicitly.
`aoide.secrets.members` names which users join `aoide-secrets-access`; no
sudo rule ships, so every admin command (`add`/`rm`/`grant`/`revoke`/
`enroll`/`set-totp`/`automate`/`expose`/`allow-remote-origin`/`migrate`) runs by hand as
`sudo -u aoide-secrets aoide secrets …`, refused outright — root included —
if the invoking uid doesn't own the secrets home. The service's own `path`
carries `bash`, `coreutils`, and `age` (the backend templates shell out to
all three); `qrencode` and `zenity` are packaged for the operator's own
shell (QR rendering for `secrets enroll`, and the popup surface, gated on
the quickshell facet being enabled).

The deployed service runs with `ProtectHome=true` — the broker has no
business reading any operator's home directory — which is why the events
feed `secrets watch` reads lives beside the broker's own socket under
`/run` rather than under the operator's home: a write there is unaffected
by `ProtectHome`, where a write into `$AOIDE_ROOT/log` (default
`~/.aoide/log`, still under home) would silently be blocked. The mirrored
audit-log write still happens — the unit exports `AOIDE_ROOT`, so the
append resolves to `$AOIDE_ROOT/log` — for the audit trail; only the
events feed `watch` tails moved off it.

## Related

- [[Agent-Interface]]
- [[Governance]]
- [[A2A-Door]]
- [[Session-Graph]] — the sealed session credential the origin gate consumes
- [[aoide-cli]]
- [[Secrets-Commands]]
- [[CLI-Reference]]
