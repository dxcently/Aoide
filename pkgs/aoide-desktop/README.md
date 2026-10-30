# aoide-desktop

The desktop app for Aoide: a Tauri 2 window over the orchestration the terminal
already drives. This directory is its own Cargo workspace, with its own lock;
AGENTS.md holds the rule that keeps it apart from the core lock. Its crates are
GUI-free libraries; the window is a later crate.

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

The window crate will call the shell. The shell sits on the door and exec
crates. The door depends on nothing else in this directory, and exec depends
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
