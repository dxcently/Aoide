# aoide-desktop

The desktop app for Aoide: a Tauri 2 window over the orchestration the terminal
already drives. This directory is its own Cargo workspace, with its own lock;
AGENTS.md holds the rule that keeps it apart from the core lock. Its crates are
GUI-free libraries; the window is a later crate.

## Crates

- `crates/door` (`aoide-desktop-door`): the loopback door client. See
  `crates/door/README.md`.
- `crates/exec` (`aoide-desktop-exec`): the one crate that starts a process or
  sends a signal. It is also the one that depends on aoide-protocol, and it
  re-exports that crate's `untrusted` module, the app's only way to it. See
  `crates/exec/README.md`.
- `crates/shell` (`aoide-desktop-shell`): the app's own doors and the asks they
  raise, over the door and exec crates. See `crates/shell/README.md`.

The window crate will call the shell. The shell sits on the door and exec
crates. The door depends on nothing else in this directory, and exec depends
on nothing else in it either.

## The gate

The deciding run has two parts. `nix flake check`'s `pkg-aoide-desktop` builds
this directory from the committed tree, lints it with clippy, and runs every
test of its workspace except the names listed in `checkFlags` in
`default.nix`; those tests fail in the sandbox, so the lane's gate, `dk.sh`,
runs them. The config test accepts one cargo config, the file nixpkgs' cargo
setup hook writes at the build's top directory to point cargo at the vendored
crates, and only when its text equals a golden template once store paths and
the vendor directory are replaced. A `dk.sh` run is a pre-check: it may read a
stale dep-info file and proves nothing about the commit.
