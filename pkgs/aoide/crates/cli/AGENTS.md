# AGENTS.md — aoide-cli

## Native Windows: what the tests may assume

The `aoide` binary this crate builds IS the fixture several other crates' tests
need, so a native build here is load-bearing beyond this crate: `cargo build --bin
aoide` must succeed on the host, and the fixture that finds it
(`aoide_test_support::built_aoide_bin`) spells its name with that host's
`EXE_SUFFIX` — a bare `aoide` is a name Windows does not have, and a fixture that
used it silently never launched the real hook process, leaving the daemon's
peer-pid verification unexercised.

The door integration tests (`daemon_dispatch_door`) exercise the real thing on
this host: a real `aoide session hook` child, the daemon stamping `__daemon-peer-pid`
from the connecting socket's own pid, and the hook's `AOIDE_SESSION_ID` claim
verified against that pid's real ancestry. `graph_residency_p_d6` re-execs this
test binary as a resident `run_loop` child; its readiness signal must be the
daemon's own startup line in the events feed, not a successful connect —
`listen()` happens inside the bind, so a connect proves only that the socket
exists, and the first dispatch then sits past the hop's 2 s reply bound (measured
on Windows as `WSAETIMEDOUT`). Those six tests serialize on
`aoide_test_support::env_lock`; a fully parallel run of that binary is still
contention-sensitive on a busy box, so read its result single-threaded.

Gated with reasons, never stubbed: the two `#!/bin/sh` plugin-shim tests (a
`CreateProcess`-invisible fixture) and the two symlink-privilege
`register_clone` tests (`SeCreateSymbolicLinkPrivilege`).

## Invariants

- **This is core. It never depends on `aoide-song`/`aoide-screen`.** Adding
  either dependency here reintroduces the graphical surface P-A5 removed
  (golden 87 → 48) — a headless box must be able to build/run this crate
  with no wayland/image/nix weight anywhere in its tree.
- **`commands::all()`'s order is byte-stable.** It reproduces the historical
  `schema.rs` table order; `schema --json` and the MCP tool list must never
  reorder. Append new `register()` calls, never reorder existing ones — see
  `pkgs/aoide/crates/AGENTS.md`.
- **`meta`/`stubs`/`onboard`/`infra` stay here, not in a domain crate.** They
  exist because they read the fully-ASSEMBLED registry (tool count, schema
  dump, `onboard`'s own call into `hooks.install` and the closing guide) — a
  domain crate can't do that without depending on this crate, which would
  invert the DAG.
- **Nix-independent.** No nix shell-outs, no NixOS assumption, anywhere in
  this crate or what it depends on (root `AGENTS.md`, "core is
  nix-independent"). Only `lyra` may be nix-dependent.
- **The audit copy of an outcome is not the outcome.** `dispatch` writes one
  line per dispatch for both doors, and its message is deliberately NOT
  always `Outcome.message`: the pairing ceremony's text carries a
  confirmation/reply code (and whatever an operator typed at that door), and
  `$AOIDE_ROOT/log` is read back (the conductor's LOG panel tails it). Keep
  `audit_message` keyed on the command PATH, never on the message's shape —
  no `NNN-NNN` scan, no "looks like a code" guess: a malformed or re-spaced
  code must be withheld exactly like a well-formed one, and a typed id is no
  different. Never redact by mutating `Outcome.message`; only the stored copy
  is withheld.

- **`aoided` starts only on a proven launch.** Its argv is parsed before
  `migrate_root_once` or any bind; an unknown argument is a refusal, never a
  start (`tests/aoided_argv.rs` asserts nothing is created). A new `aoided`
  flag extends `parse` and the usage page together.
- **SIGPIPE is default in `aoide`, ignored in `aoided`.** The daemon writes to
  peers that vanish and must see EPIPE as an `io::Error`, never die of a signal.

## Extension points

- **A new root-coupled command** (one that must read the assembled
  registry) adds a case to `meta`/`infra`, or its own dedicated module
  (`onboard`'s precedent) when the command is substantial enough to warrant
  one; anything else belongs in its domain crate's own `commands` module
  instead.
- **A new special-cased command** (bypassing the generic `Outcome` envelope)
  extends the `special` closure passed to `aoide_protocol::door::run` in
  `run_cli`.

## Docs update required in the same commit

- This `README.md` when the command count, a root-coupled group, or a
  special-cased command changes.
- The golden snapshot in `registry.rs` when the command-path set changes.
- `docs/architecture/PACKAGE-LAYOUT.md`/`CONTRACTS.md §3` when the
  core/lyra split itself shifts.
