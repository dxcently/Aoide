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
