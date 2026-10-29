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

- The deciding run has two parts. First, `nix flake check`'s `pkg-aoide-desktop`
  builds this directory from the committed tree, in its own sandbox: it lints
  the product configuration, then the gate's form, with `-D warnings`
  (`preCheck`), and it runs every test of the workspace with every feature
  (`checkPhase`), except the names listed in `checkFlags` in `default.nix`.
  Second, the lane's gate, `dk.sh`, runs the listed tests. Lane test runs happen
  in snowglobe VMs, never on the host. A `dk.sh` run is a pre-check: it may read
  a stale dep-info file that an earlier build left beside the test binary, and
  it proves nothing about the commit.
- The listed names are the tests that fail in the sandbox, whose `/` belongs to
  uid 65534, which the exec crate's pin rule refuses. Each name matches exactly
  (`--exact`), so a test that shares a listed name is skipped too, and a test
  not on the list is never skipped. A new test whose spawn reaches exec's pin
  rule, and that is not on the list, turns the sandbox run red.
- The lints are the guard. The root manifest's `[workspace.lints]` forbids
  `unsafe_code`, `disallowed_methods` and `disallowed_types`. Door and shell
  take that table with `[lints] workspace = true`, so no attribute in either
  can lift the three, however it is spelled: rustc refuses the lift with
  E0453. The `unsafe_code` forbid also refuses `#[no_mangle]`, `#[export_name]`,
  `#[link_section]` and `global_asm!`, so no door or shell code defines a symbol
  that a libc call in exec could reach. Exec's own table allows the three, so
  exec is the one crate where `unsafe`, a process start or a signal can appear.
- `clippy.toml` lists the std paths that start a process or send a signal:
  `Command` (the type), `Command::new`, `CommandExt::pre_exec`,
  `CommandExt::exec` and `Child::kill`, which sends SIGKILL. In door and shell,
  a call through an alias of any of them fails the clippy run. It lists no libc
  path, because every libc function is `unsafe` to call, so the `unsafe` forbid
  already stops a libc call in door and shell. The gate compares `clippy.toml`
  with its golden text in `gates.rs`, so a path dropped from it, or a missing
  file, fails (`the_manifests_are_the_golden_ones`).
- The forbids are errors on their own, and `-D warnings` makes every other
  warning fail the gate too. The pre-check form of the second clippy pass is
  `dk.sh clippy --offline --workspace --all-targets
  --all-features -- -D warnings`.
- The text scan is a tripwire, not the guard. `crates/shell/tests/gates.rs`
  reads every `.rs` file under this directory, and every file under `ui/` once
  that exists, skipping the root's `target`. It bars six words in each one:
  `aoided.sock`, `"dispatch"`, `"subscribe"`, `/api/launch`, `sh -c` and
  `SIGKILL`. It reads the raw text, so a split spelling passes it. The scan
  reads source text, not expansions, so it does not see a lift a macro writes;
  the forbid refuses one that a `macro_rules!` macro writes; a proc macro comes
  only from a new member or a changed manifest, which moves a golden manifest,
  or from a dependency, which is not linted.
- The config and member checks, in the same file:
  - no `build.rs` lies under this directory (`no_member_has_a_build_script`);
  - no symbolic link lies under this directory, since a walk through one
    could read outside it (every test that walks the tree);
  - the root's `clippy.toml` is the only clippy config: a `clippy.toml` below
    the root, or a `.clippy.toml` anywhere, fails
    (`only_the_roots_clippy_config_is_read`);
  - no `.cargo/config` or `.cargo/config.toml` lies in this directory, in any
    ancestor up to `/`, or in the cargo home (`$CARGO_HOME`, or `$HOME/.cargo`
    when that is unset), with one exception: the file
    `$NIX_BUILD_TOP/.cargo/config.toml`, which the pinned nixpkgs' cargo setup
    hook writes to point cargo at the vendored crates. The test accepts that
    file only when its text equals
    `crates/shell/tests/fixtures/nix-cargo-config.toml` once every store path
    (a `/nix/store/` hash and name, in nix's base-32) and the vendor directory
    `cargo-vendor-dir`, the relative value the hook writes, are placeholders.
    The golden fixes the linker line too
    (`no_cargo_config_applies_to_the_workspace`);
  - exec's only `.rs` file is `src/lib.rs`, because cargo makes a target of any
    `.rs` under `src/bin`, `tests`, `examples` or `benches`, and exec's table
    allows the lints in each (`the_exec_crate_has_one_source_file`);
  - the files rustc read to build exec are its `src/lib.rs`, the root's
    `clippy.toml` and `Cargo.toml`, and exec's `Cargo.toml`, by the dep-info
    files cargo leaves beside the test binary. Each of those files lists
    `src/lib.rs`, and each of its lines is a target in that directory, a
    `<file>:` line, or an `# env-dep:` line; any other line reds
    (`the_exec_crate_compiles_only_its_lib_rs`). A `#[path]` module or an
    `include!` in exec would compile another file under exec's table; in door
    and shell, such a file compiles under the forbid. The read misses a build
    script or a proc macro of a dependency, a linked library, and code that
    only a build the gate does not make compiles, such as one without `fakes`;
  - exactly four `Cargo.toml` files lie under this directory, and each equals
    its golden text in the same file, less blank lines and comment lines
    (`the_manifests_are_the_golden_ones`). A new member brings its golden in
    the same commit.
- The scan reads nothing outside this directory but those dep-info files and
  the nix build's cargo config, which it reads only to compare with the golden
  template. A target whose path lies outside it would change a golden manifest.
- Dependencies are not linted. The lints and the scan cover the members only.
  `aoide-protocol` is a path dependency outside this workspace, and it starts
  processes of its own. The app reaches it only through the exec crate's
  `untrusted` re-export.
- A `dk.sh` run's environment is set by `dk.sh`, and the deciding run's is the
  nix sandbox's. `dk.sh` runs cargo under `nix develop -i -k PATH`, so the tests
  see only the dev shell's variables and PATH. It then unsets
  `AOIDE_SESSION_ID`, `AOIDE_ROOT`, `LIBRARY_PATH`, `RUSTFLAGS`,
  `CARGO_ENCODED_RUSTFLAGS`, `CARGO_BUILD_RUSTFLAGS`, the x86_64 Linux target's
  `RUSTFLAGS`, `CLIPPY_CONF_DIR`, `RUSTC_BOOTSTRAP`, `RUSTC_WRAPPER`,
  `RUSTC_WORKSPACE_WRAPPER`, `CARGO_BUILD_RUSTC_WRAPPER`,
  `CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER`, `RUSTC` and `CARGO_BUILD_RUSTC`, and it
  sets `HOME`, `CARGO_HOME`, `CARGO_BUILD_JOBS`, `TMPDIR`, `XDG_RUNTIME_DIR` and
  `CARGO_TARGET_DIR`. Nothing scans the environment.
