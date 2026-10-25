# AGENTS.md: aoide-desktop

The invariants that span this directory's crates. The layout is in README.md,
and each crate's own invariants are in its AGENTS.md (`crates/door/AGENTS.md`,
`crates/shell/AGENTS.md`).

## The workspace

- This directory is its own Cargo workspace, with its own `Cargo.lock`. It is
  never a member of `pkgs/aoide`'s workspace, so GUI crates never reach the core
  lock, and nothing here moves it.
- The shared requirements (serde, serde_json, libc, sha2) live in
  `[workspace.dependencies]`, with the requirements `pkgs/aoide` uses.
- Every cargo command runs with `--offline`.

## Tests

- Use the fakes only: the door's `fake` feature, and the shell's `fakes`
  feature for its fake aoide and fake eidolon. Never the installed `eidolon` or
  `aoide`, and never a GUI.
- The fake aoide and the fake eidolon are bins with `required-features =
  ["fakes"]`, so the exec and door tests need `--all-features`.
- The token, client and exec tests create directories under `TMPDIR`, so run
  them with `TMPDIR` set to a directory the tests may write.
- No test sets a process-wide environment variable. A test that needs a path
  passes it as a parameter.

## The gate

- The gate has two mechanisms, and the lints are the main one. The workspace's
  `[workspace.lints]`, in this directory's `Cargo.toml`, denies `unsafe_code`,
  `disallowed_methods` and `disallowed_types` for every target, and each member
  takes that table with `[lints] workspace = true`. `clippy.toml` here lists the
  paths no code may call: the process starters (`std::process::Command`,
  `libc::fork`, `libc::execv`, ...), the signal senders (`libc::kill`, and
  `std::process::Child::kill`, which sends SIGKILL) and `libc::syscall`. The
  gate runs clippy with `-D warnings`, and the lints are errors without it too,
  so a call through an alias of a listed path fails either way. `exec.rs` holds
  the one allow for those lints.
- The text scan is the backstop, over the workspace's own sources. The
  `crates/shell/tests/gates.rs` test reads every `.rs` under this directory,
  skipping any `target` directory, and every file under `ui/` once that
  directory exists. It forbids `aoided.sock`, `"dispatch"`, `"subscribe"`,
  `/api/launch`, `sh -c`, SIGKILL, `include!(` and `#[path`. Which file may
  hold which word is in `crates/shell/AGENTS.md`, with the whole-word rule. It
  also checks that the workspace denies the three lints and that each member
  takes them.
- Residual: a target whose file lies outside this directory is not read by the
  scan, though its crate's lints still apply to it.
- Dependencies are not scanned, `aoide-protocol` included. The app calls only
  `untrusted`'s functions from it, and the protocol crate's own gates cover the
  rest of it.
