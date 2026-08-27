---
type: concept
created: 2026-08-25
updated: 2026-08-27
tags: [aoide, cli, secrets, totp, security]
---

# Secrets Commands — the `aoide secrets` Command Surface

The dev-facing reference for the 16 `secrets` commands: signature, what each
reads and writes, and where output goes. This page is the command surface;
[[Secrets-Broker]] is the design — the identity/release-to-client flow, the
policy model's rationale, the parked-ask design, the age lane, deployment.
Handlers live in `pkgs/aoide/crates/secrets/src/commands.rs`; the broker
daemon in `broker.rs`; the admin mutation logic (shared by the direct-write
and socket paths) in `admin.rs`.

## Shared facts

The secrets home is `/var/lib/aoide-secrets` (`$AOIDE_SECRETS_HOME`), mode
`0700`: `policy.json`, `totp.secret`, the replay ledger, and the backend
store files. The broker's unix socket is `$AOIDE_SECRETS_SOCKET` else
`/run/aoide-secrets/secrets.sock`, mode `0660`, group
`aoide-secrets-access` — the only door onto the home. A separate
broker-owned events feed backs `secrets watch`: `$AOIDE_SECRETS_EVENTS`
else a sibling of the socket, `/run/aoide-secrets/events.jsonl`, capped at
1 MiB (truncated in place past the cap).

**Three routing classes**, the page's organizing idea:

- **Direct-home admin** — `add`, `rm`, `grant`, `revoke`, `set-totp`,
  `automate`, `expose`, `migrate`. Each refuses outright when the calling
  process's effective uid doesn't own the secrets home, before ever
  touching `policy.json`/`totp.secret` — root included; plain `sudo`
  (euid 0) is refused the same as any other wrong uid, only `sudo -u
  aoide-secrets` passes. When a broker is listening, every one of these
  eight commands routes over the socket FIRST (`{"op":"admin","command":…}`),
  executed inside the same `put_lock` critical section a `put`/`exec`
  already runs under — so an admin command racing a live resolve can no
  longer interleave. Direct-write-to-`policy.json` survives only as the
  no-daemon fallback, reached only when the socket connect itself fails
  with `ENOENT`/`ConnectionRefused`; any other socket outcome, including
  the broker's own denial, is reported straight through, never silently
  downgraded into the fallback. Both paths call the identical `admin.rs`
  functions — one implementation, two gates.
- **Over-the-socket operator** — `put`, `exec`, `pending`, `approve`,
  `dismiss`, `watch`. CLI-door-only; no admin-identity check (`pending` in
  particular touches only in-memory broker state, no `policy.json` write).
- **The daemon itself** — `serve`.

None of the 16 carries `gated: true` in the schema. `secrets exec` on a
`requireTotp` secret with no code PARKS instead of refusing outright: the
requesting connection registers `{id, secret, consumer, requestedAt}` in
the broker's in-memory registry and waits (default 300 s,
`AOIDE_SECRETS_PARK_TIMEOUT`) until a separate connection completes or
dismisses the ask, or the timeout elapses. Audit is broker-side only —
`broker::audit_resolve`/`audit_put`/`audit_admin` write both the broker's
own `audit.log` and the mirrored `$AOIDE_ROOT/log` (default `~/.aoide/log`),
before a value is ever
released; a secret's value never appears on a `Serialize` type in the
crate, so it can never reach an `Outcome`, a JSON field, argv, or a log
line.

## aoide secrets serve

```
aoide secrets serve [--json]
```

- **Reads:** nothing beyond the resolved home/socket paths.
- **Writes:** binds the socket (chmods it `0660` on bind); seeds
  `backends.json` with the built-in `file`/`age` backend entries on a
  fresh home, then backfills any missing built-in entry into an existing
  `backends.json` (never touching an already-present entry, built-in or
  custom).
- **Output:** long-running — blocks serving the socket. This record is the
  launch's own audit line, the same shape `a2a serve`'s launch record is.
- **Notes:** not gated. Thread-per-connection: a parked `exec` blocks only
  its own connection's thread, so every other caller is served normally
  the entire time one ask sits parked.

## aoide secrets exec

```
aoide secrets exec [--as <consumer>] [--secret <name[:VAR]>] [--totp <code>] [--json] -- <cmd>
```

- **Reads:** `--secret <name>` (optionally `name:VAR` to name the injected
  env var; default the name, uppercased, `-` → `_`); resolves through the
  broker's `resolve` wire op.
- **Writes:** nothing.
- **Output:** `Stdio::inherit` throughout — the child's stdio passes
  through untouched; the resolved value is injected as an env var and the
  command execs, returning the child's own exit code. The value exists only
  inside this client process's own environment, from the socket reply to
  the exec — never argv, never an `Outcome`/JSON field, never either audit
  log.
- **Notes:** CLI-only — the value would otherwise have to cross a door
  that isn't this process's own stdio. `--totp` is verified live against
  this host's enrollment (`±1`-timestep window, single-use). Omitted (or
  wrong) on a `requireTotp` secret PARKS the resolve rather than refusing
  outright; complete it with `secrets pending`/`secrets approve <id>
  --totp <code>` from another terminal, or wait out the timeout.

## aoide secrets add

```
aoide secrets add <name> [--backend <name>] [--key <key>] [--require-totp] [--consumers <a,b,…>] [--json]
```

- **Reads:** the existing policy set (to refuse a duplicate name).
- **Writes:** `policy.json` — a new policy entry: name, backend, key,
  `consumers[]`, `requireTotp`. Never a value; the broker never stores
  one at registration time.
- **Output:** `data: {path: "broker"|"direct"}` naming which write path
  ran, alongside the usual message/changed fields.
- **Notes:** `--backend` defaults to `age` (the built-in age-encrypted
  store) when omitted — an already-recorded policy's backend field is
  never touched by this default; only a brand-new `add` with the flag
  omitted is affected. Direct-home admin command (see "Shared facts").

## aoide secrets rm

```
aoide secrets rm <name> [--json]
```

- **Reads/Writes:** removes the named entry from `policy.json`. The
  backend's own store is untouched — this only forgets aoide's policy
  record.
- **Output:** `data: {path: "broker"|"direct"}`.
- **Notes:** direct-home admin command.

## aoide secrets grant

```
aoide secrets grant <name> <consumer> [--json]
```

- **Reads/Writes:** adds one consumer to the named policy's `consumers[]`.
- **Output:** `data: {path: "broker"|"direct"}`.
- **Notes:** direct-home admin command.

## aoide secrets revoke

```
aoide secrets revoke <name> <consumer> [--json]
```

- **Reads/Writes:** removes one consumer from the named policy's
  `consumers[]`. A revoke issued while an `exec` ask for that secret sits
  parked still denies it: `approve` re-runs the full authorization gate
  against the ask's stored consumer at release time, not merely at the
  moment it parked.
- **Output:** `data: {path: "broker"|"direct"}`.
- **Notes:** direct-home admin command.

## aoide secrets enroll

```
aoide secrets enroll [--force] [--show] [--json]
```

- **Reads:** with `--show`, the existing enrollment; otherwise none.
- **Writes:** `totp.secret` (a fresh 20-byte secret from `/dev/urandom`,
  `0600`) and resets the replay ledger — only on a first enrollment or
  `--force`.
- **Output:** prints the `otpauth://` URI + base32 form directly to
  stdout (plus a QR code when `qrencode` is on `PATH`) — never through the
  envelope, so the secret material never rides a JSON field.
- **Notes:** one enrollment per host. `--force` rotates (old codes stop
  working immediately); `--show` reprints the current enrollment without
  rotating anything; the two flags are mutually exclusive. CLI-only,
  admin-identity gated.

## aoide secrets put

```
aoide secrets put <name> [--force] [--json]
```

- **Reads:** stdin — hidden-prompt on a tty (raw termios, restored
  unconditionally even on a read error), byte-identical piped-through
  read otherwise. Never argv.
- **Writes:** the named secret's value via its backend's `set` template,
  run as the secrets uid — only when the policy's backend carries a `set`
  template (both built-in backends do).
- **Output:** ok → which happened, "stored" a new value or "replaced" an
  existing one. Overwrite refused unless `--force`: the broker-side
  existence probe (the same `get` template `resolve` uses) returns the
  machine-readable `{"exists":true}` reply — never inferred from error
  text — which a tty turns into a `y/N` confirmation and a piped stdin
  turns into a taught `--force` hint.
- **Notes:** CLI-only, admin-side — no `consumer` field, never
  TOTP-gated. `secrets add` must register the policy first; `put` never
  auto-creates one.

## aoide secrets set-totp

```
aoide secrets set-totp <name> on|off [--json]
```

- **Reads/Writes:** flips the named policy's `requireTotp` bit directly in
  `policy.json`.
- **Output:** idempotent — re-setting the same state reports "unchanged"
  and writes nothing; `data: {path: "broker"|"direct"}` otherwise.
- **Notes:** direct-home admin command.

## aoide secrets automate

```
aoide secrets automate <name> on|off|grant|revoke [<consumer>] [--json]
```

- **Reads/Writes:** the named policy's `automation: {enabled,
  consumers[]}` field — `on`/`off` flips `enabled`; `grant`/`revoke <consumer>`
  edits the `consumers[]` list.
- **Output:** idempotent — re-flipping the same state, or granting/revoking
  a consumer already in/out of the list, reports "unchanged" and writes
  nothing.
- **Notes:** direct-home admin command. Can only ever relax `requireTotp` for
  the listed consumers, never impose it on a policy that doesn't already
  carry it. Checked against the same self-asserted `consumer` wire field
  every other gate trusts — an automation-open secret is effectively
  code-free for any local socket caller claiming a listed name.

## aoide secrets expose

```
aoide secrets expose <name> on|off [--json]
```

- **Reads/Writes:** the named policy's `remote` bit (default `false`).
- **Output:** idempotent, same discipline as `set-totp`.
- **Notes:** direct-home admin command. No behaviour change today — no
  non-local entry point onto the broker exists yet; the bit exists so an
  operator can pre-declare which secrets are meant to ever leave the host,
  ahead of one landing.

## aoide secrets pending

```
aoide secrets pending [--json]
```

- **Reads:** the broker's in-memory park registry — never a file.
- **Output:** every parked ask: `{id, secret, consumer, requestedAt,
  peerUid}` — `peerUid` is the kernel-truth `SO_PEERCRED` uid stamped on
  the ask's original connection (`null` when unidentified) — never a
  value.
- **Notes:** over-the-socket operator command, but NOT an admin/euid command —
  it only reads in-memory state, no `policy.json` write.

## aoide secrets approve

```
aoide secrets approve <id> [--totp <code>] [--json]
```

- **Reads:** the parked ask named `<id>` (from `secrets pending`).
- **Writes:** releases the value down the ORIGINAL requesting connection,
  never into this command's own reply. Re-runs the full authorization gate
  (exists + consumer authorized) against the ask's stored consumer before
  fetching, so a revoke issued mid-park still takes effect.
- **Output:** ok → `{"ok":true}` to the approver; the value itself never
  rides this reply. An invalid or expired code leaves the ask parked and
  the replay ledger unburned.
- **Notes:** over-the-socket operator command.

## aoide secrets dismiss

```
aoide secrets dismiss <id> [--json]
```

- **Reads:** the parked ask named `<id>`.
- **Writes:** removes it from the registry.
- **Output:** the original requesting connection gets a clean "dismissed"
  refusal, no code needed.
- **Notes:** over-the-socket operator command. Peer-uid-gated: an ordinary
  caller may only dismiss an ask whose stamped `peerUid` matches its own
  connection's peer uid; the broker's own effective uid may dismiss any
  ask.

## aoide secrets watch

```
aoide secrets watch [--json] [--popup]
```

- **Reads:** tail-follows the broker-owned events feed from EOF
  (`stat(2)` once a second; reopens at 0 on truncation or a broker
  restart). `secrets pending`'s own poll of the in-memory registry is the
  authority — the tail reconciles against it once at startup and again on
  every event plus a 30-second safety tick.
- **Writes:** nothing.
- **Output:** narrates five event kinds — `released` (a TOTP-free grant),
  `parked`, `completed`, `dismissed`, `expired`. On a terminal, a parked
  ask prompts inline (`[a]` approve with a hidden code, `[d]` dismiss,
  `[i]` ignore for this session), FIFO by expiry. `--json` emits one event
  object per line.
- **Notes:** CLI-only, operator-side, blocks until Ctrl-C. `--popup`
  swaps the terminal prompt for a `zenity --entry --hide-text` dialog,
  parked-only, unlock-gated (`loginctl`'s `LockedHint` OR'd with an
  `AOIDE_SECRETS_LOCKER`-named `/proc` scan, default `hyprlock`); a code
  prompt refuses to open below 10 seconds remaining and force-closes at
  15 seconds without reopening. `--json` and `--popup` are mutually
  exclusive.

## aoide secrets migrate

```
aoide secrets migrate <name> [--backend <target>] [--json]
```

- **Reads:** the named secret's current value via its policy's current
  backend.
- **Writes:** stores the value on the target backend (default `age`)
  first, flips and saves the policy's `backend` field second, removes the
  old value LAST — strictly after the policy flip has already succeeded,
  so a failure at any earlier step leaves the secret resolvable on its
  original backend.
- **Output:** reports which happened; old-value removal only ever runs
  for a built-in source backend (`file`/`age`) whose on-disk path this
  crate can derive on its own — any other source is left untouched, and
  the migration reports that plainly.
- **Notes:** direct-home admin command, no socket round trip — the value
  lives only as a local `String` inside the handler, never crossing a
  wire.

## The A2A seam

`aoide a2a serve --bearer-secret <name>` and `aoide peer add
--bearer-secret <name>` both resolve through this broker on every
connection/call, self-asserted consumers `a2a-door`/`a2a-client`
respectively, via `aoide_secrets::client::resolve_bounded` — a short
bounded read (`wait:false` on the wire, so a misconfigured `requireTotp`
secret refuses immediately rather than parking the door) with no caching.
A broker resolve failure fails closed: every bearer check on that
connection is denied. See [[Doors-and-Peers]] for the two flags'
per-command detail.

## Related

- [[Secrets-Broker]]
- [[Doors-and-Peers]]
- [[A2A-Door]]
- [[Governance]]
- [[aoide-cli]]
- [[CLI-Reference]]
