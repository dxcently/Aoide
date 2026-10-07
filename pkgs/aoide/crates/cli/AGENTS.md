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
- **`do` never runs what it prints.** It is a registry entry like any other
  and holds no new power: the classifier's verdict is a candidate, the bound
  line is checked by `Command::check` before it is shown, and a verdict that
  argues against dispatch (`accept` not true, `conflicts`, trailing text,
  `none`, an intent or slot the registry lacks) is a taught refusal, never a
  best guess. The printed line must re-parse (`door::parse`) to the exact checked
  invocation, and a verdict is read only from a child that exited 0. The
  denied intents (`vv/kit.rs::DENIED`: secrets, mesh charter, irreversible
  removals, trust/grant/config changes, pair, server modes) stay out of the kit and out of `resolve`. Do not add an execute path, a `--run` flag, or a fallback that
  picks the top candidate; `docs/architecture/AOIDE-VV-JEV.md` is the table.
  The classifier is a shell-out (`verba-volantia` on `PATH` or `[verba]
  binary`) — never a VV crate, candle, or vendored weights in this tree.
- **The kit is derived from the registry, never authored beside it.**
  `vv::kit` is the only place intent ids and slot names are made, and
  `kit::resolve` is the only place they are read back. A command's phrasings
  come from its own path, brief, summary and examples, so a command that wants
  better recognition improves its `brief`/`examples`, not a second list here.
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
- **SIGPIPE stays ignored in BOTH binaries.** Writes to a child's stdin
  (`secrets` backends, `curl`, `qrencode`) must return their error, not kill
  the process. A closed stdout (`aoide … | head`) is handled where the result
  is printed: `door::say`/`say_raw` map BrokenPipe to a quiet exit 141. Print a
  result through them, never `println!`.

## Extension points

- **A new root-coupled command** (one that must read the assembled
  registry) adds a case to `meta`/`infra`, or its own dedicated module
  (`onboard`'s precedent) when the command is substantial enough to warrant
  one; anything else belongs in its domain crate's own `commands` module
  instead.
- **A command that changes the closed set** (a new, renamed or removed
  command) needs no edit to `vv`: `aoide do kit` re-derives the spec, and a
  kit trained before it is refused by name for an intent the registry no
  longer holds. Retrain after a command-set change that matters to `do`.
- **A new special-cased command** (bypassing the generic `Outcome` envelope)
  extends the `special` closure passed to `aoide_protocol::door::run` in
  `run_cli`.

## Docs update required in the same commit

- This `README.md` when the command count, a root-coupled group, or a
  special-cased command changes.
- `docs/architecture/AOIDE-VV-JEV.md` and `CONTRACTS.md §3` when `do`'s
  refusal table, its wire reads, or the kit's shape changes; `[verba]` is
  `aoide_storage::config`'s and CONTRACTS §4's.
- The golden snapshot in `registry.rs` when the command-path set changes.
- `docs/architecture/PACKAGE-LAYOUT.md`/`CONTRACTS.md §3` when the
  core/lyra split itself shifts.
