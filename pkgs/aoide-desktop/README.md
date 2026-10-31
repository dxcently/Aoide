# aoide-desktop

The desktop app for Aoide: a Tauri 2 window over the orchestration the terminal
already drives. This directory is its own Cargo workspace, with its own lock;
AGENTS.md holds the rule that keeps it apart from the core lock. Its crates are
GUI-free libraries; `crates/app` is the one window and the one binary.

## Crates

- `crates/door` (`aoide-desktop-door`): the loopback door client. See
  `crates/door/README.md`.
- `crates/exec` (`aoide-desktop-exec`): starts a process or sends a signal, and
  so does its path dependency aoide-protocol. It is the one member that depends
  on aoide-protocol, and it
  re-exports that crate's `untrusted` module, the app's only way to it. See
  `crates/exec/README.md`.
- `crates/shell` (`aoide-desktop-shell`): the app's own doors and the asks they
  raise, over the door and exec crates. See `crates/shell/README.md`.
- `crates/app` (`aoide-desktop-app`): the one window over the shell, and the
  `aoide-desktop` binary. Its six Tauri commands are granted to the `main`
  window alone. See `crates/app/README.md`.

The app crate calls the shell. The shell sits on the door and exec crates. The door depends on nothing else in this directory, and exec depends
on nothing else in it either.

## The gate

The deciding run is `nix flake check`'s `pkg-aoide-desktop`. It builds this
directory from the git-tracked files, dirty ones included, lints it with clippy,
and runs every test of its workspace except the names listed in `checkFlags` in
`default.nix`. No pipeline runs those listed tests yet. The config test accepts
one cargo config, `.cargo/config.toml` at the build's top directory, which
nixpkgs' cargo setup hook writes to point cargo at the vendored crates. It
accepts that file only when its text equals a golden template once store paths
and the vendor directory are replaced. A run outside the sandbox is a pre-check:
it may read a stale dep-info file and proves nothing about the commit.

## The one fork

`vendor/wry` is wry 0.57.0 with one block in `create_context`
(`src/webkitgtk/web_context.rs`): four comment lines and
`context.set_sandbox_enabled(true);`. webkit2gtk-4.1 runs its page process
unsandboxed unless that call runs before the first WebView exists, and wry
creates that WebView itself, so the call has to sit in wry. The root
`Cargo.toml`'s `[patch.crates-io]` points wry at this copy, and the lock's wry
entry is the path package.

The fork goes when wry or tauri offers the option. Until then, a wry bump is a
re-sync: copy the new registry tree over `vendor/wry`, add the block back, and
move the digest constant `WRY_0_57_0_DIGEST` in `crates/shell/tests/gates.rs`
to the new upstream digest. The digest covers every file of the tree, with the
block taken out of `web_context.rs`, so a copy that passes equals upstream in
every other byte. The test's name carries the version and moves with it.
