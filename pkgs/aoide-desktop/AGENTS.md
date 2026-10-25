# AGENTS.md: aoide-desktop

The invariants that span this directory's crates. The layout is in README.md,
and each crate's own invariants are in its AGENTS.md (`crates/door/AGENTS.md`,
`crates/exec/AGENTS.md`, `crates/shell/AGENTS.md`).

## The workspace

- This directory is its own Cargo workspace, with its own `Cargo.lock`. It is
  never a member of `pkgs/aoide`'s workspace, so GUI crates never reach the core
  lock, and nothing here moves it.
- The shared requirements (serde, serde_json, libc, sha2) live in
  `[workspace.dependencies]`, with the requirements `pkgs/aoide` uses, except
  sha2: this workspace pins 0.10, where `pkgs/aoide` pins 0.11.
- Every cargo command runs with `--offline`.

## Tests

- Use the fakes only: the door's `fake` feature, and the shell's `fakes`
  feature for its fake aoide and fake eidolon. Never the installed `eidolon` or
  `aoide`, and never a GUI.
- The fake aoide and the fake eidolon are the shell's bins, with
  `required-features = ["fakes"]`. The door's client tests need the door's
  `fake` feature, and the exec crate's `make_fifo` and `test_doors` need
  `fakes`, so those tests run with `--all-features`.
- The token, client and exec tests create directories under `TMPDIR`, so run
  them with `TMPDIR` set to a directory the tests may write.
- No test sets a process-wide environment variable. A test that needs a path
  passes it as a parameter.

## The gate

- The lints are the guard. The root manifest's `[workspace.lints]` forbids
  `unsafe_code`, `disallowed_methods` and `disallowed_types`. Door and shell
  take that table with `[lints] workspace = true`, so no attribute in either
  can lift the three, however it is spelled: rustc refuses the lift with
  E0453. Exec's own table allows the three, so exec is the one crate where
  `unsafe`, a process start or a signal can appear.
- `clippy.toml` lists the std paths that start a process or send a signal:
  `Command` (the type), `Command::new`, `CommandExt::pre_exec`,
  `CommandExt::exec` and `Child::kill`, which sends SIGKILL. In door and shell,
  a call through an alias of any of them fails the clippy run. It lists no libc
  path, because every libc function is `unsafe` to call, so the `unsafe` forbid
  already stops a libc call in door and shell.
- The gate runs clippy as `dk.sh clippy --offline --workspace --all-targets
  --all-features -- -D warnings`. The forbids are errors on their own, and
  `-D warnings` makes every other warning fail the gate too.
- The text scan is a tripwire, not the guard. `crates/shell/tests/gates.rs`
  reads every `.rs` file under this directory, and every file under `ui/` once
  that exists, skipping the root's `target`. It bars six words in each one:
  `aoided.sock`, `"dispatch"`, `"subscribe"`, `/api/launch`, `sh -c` and
  `SIGKILL`. It reads the raw text, so a split spelling passes it. It also
  reads source text, not expansions, so a lift a macro writes is not seen by
  it; the forbid still refuses that lift.
- The config and member checks, in the same file:
  - no `build.rs` lies under this directory (`no_member_has_a_build_script`);
  - the root's `clippy.toml` is the only one
    (`only_the_roots_clippy_config_is_read`);
  - no `.cargo/config` or `.cargo/config.toml` lies in this directory or in
    any ancestor up to the git root
    (`no_cargo_config_applies_to_the_workspace`);
  - exec's only `.rs` file is `src/lib.rs`, because cargo makes a target of any
    `.rs` under `src/bin`, `tests`, `examples` or `benches`, and exec's table
    allows the lints in each (`the_exec_crate_has_one_source_file`);
  - exactly four `Cargo.toml` files lie under this directory, and each equals
    its golden text in the same file, less blank lines and comment lines
    (`the_manifests_are_the_golden_ones`). A new member brings its golden in
    the same commit.
- The scan reads nothing outside this directory. A target whose path lies
  outside it would change a golden manifest.
- Dependencies are not linted. The lints and the scan cover the members only.
  `aoide-protocol` is a path dependency outside this workspace, and it starts
  processes of its own. The app reaches it only through the exec crate's
  `untrusted` re-export.
- The gate's environment is set by `dk.sh`. It sets `CARGO_HOME`, `TMPDIR`,
  `XDG_RUNTIME_DIR` and `CARGO_TARGET_DIR`, and it unsets `AOIDE_SESSION_ID`,
  `AOIDE_ROOT`, `RUSTFLAGS`, `CARGO_ENCODED_RUSTFLAGS`,
  `CARGO_BUILD_RUSTFLAGS`, the x86_64 Linux target's `RUSTFLAGS`,
  `CLIPPY_CONF_DIR`, `RUSTC_BOOTSTRAP`, `RUSTC_WRAPPER` and
  `RUSTC_WORKSPACE_WRAPPER`. Nothing scans the environment. `CARGO_HOME` holds
  no config, which the gate assumes, since a config there would apply to every
  build.
