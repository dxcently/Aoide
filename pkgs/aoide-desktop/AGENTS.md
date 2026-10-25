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
  `libc::fork`, `libc::clone`, `libc::execv`, ..., `libc::execvpe`), the signal
  senders (`libc::kill`, `libc::killpg`, `libc::tgkill`, `libc::sigqueue`,
  `libc::pthread_kill`, and `std::process::Child::kill`, which sends SIGKILL)
  and `libc::syscall`. The gate runs clippy with `-D warnings`, and the lints
  are errors without it too, so a call through an alias of a listed path fails
  either way. `exec.rs` holds the one allow for those lints.
- The text scan is the backstop, over the workspace's own sources. The
  `crates/shell/tests/gates.rs` test reads every `.rs` under this directory,
  skipping only this directory's own `target`, and every file under `ui/` once
  that directory exists. It forbids `aoided.sock`, `"dispatch"`, `"subscribe"`,
  `/api/launch`, `sh -c` and SIGKILL. The lint names and the forms `include!(`
  and `#[path` are matched on each source with its ASCII whitespace removed, so
  `clippy :: style` meets the needle `clippy::style`; the lint names match as
  whole words. Which file may hold which word is in `crates/shell/AGENTS.md`.
- Outside `exec.rs`, no source builds a lint or an attribute from a macro
  variable: no dollar sign after `#[`, `#![` or `clippy::`, and none in the
  arguments of an `allow`, `expect` or `cfg_attr` attribute.
- The config and member checks: the only `clippy.toml` under this directory is
  its root's, and no `.cargo/config` or `.cargo/config.toml` lies in this
  directory or in any ancestor up to the git root. The root manifest's `members`
  is one line of quoted paths, and it must name exactly the `Cargo.toml` files
  under this directory, the root's excepted. Each of those takes
  `[lints] workspace = true`, with no `[lints.` table and no `proc-macro`, and
  the root manifest holds no `[package]`.
- Residuals: a lift a proc-macro would emit is not seen, which is why no member
  may be one. A target whose file lies outside this directory is not read by the
  scan, though its crate's lints still apply to it. The gate's environment is
  not scanned: `CARGO_HOME`'s config and any `RUSTFLAGS` apply to every build.
- Dependencies are not scanned, `aoide-protocol` included. The app calls only
  `untrusted`'s functions from it, and the protocol crate's own gates cover the
  rest of it.
