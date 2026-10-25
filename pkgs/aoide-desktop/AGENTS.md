# AGENTS.md: aoide-desktop

The invariants that span this directory's crates. The layout is in README.md,
and each crate's own invariants are in its AGENTS.md (`crates/door/AGENTS.md`,
`crates/shell/AGENTS.md`).

## The workspace

- This directory is its own Cargo workspace, with its own `Cargo.lock`. It is
  never a member of `pkgs/aoide`'s workspace, and nothing here moves the core
  lock.
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

- The gate is the workspace's own: a grep test in `crates/shell/tests/gates.rs`
  that reads every `.rs` under `crates/`, and every file under `ui/` once that
  directory exists. It forbids `aoided.sock`, `dispatch`, `subscribe`,
  `/api/launch`, `sh -c` and SIGKILL, and it holds the process-starting words to
  `exec.rs`.
- Dependencies are not scanned, `aoide-protocol` included. The app calls only
  `untrusted`'s functions from it, and the protocol crate's own gates cover the
  rest of it.
