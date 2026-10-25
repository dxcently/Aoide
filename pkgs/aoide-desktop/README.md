# aoide-desktop

The desktop app for Aoide: a Tauri 2 window over the orchestration the terminal
already drives. This directory is its own Cargo workspace, with its own lock;
AGENTS.md holds the rule that keeps it apart from the core lock. Its crates are
GUI-free libraries; the window is a later crate.

## Crates

- `crates/door` (`aoide-desktop-door`): the loopback door client. See
  `crates/door/README.md`.
- `crates/shell` (`aoide-desktop-shell`): the shell is the one place in the app's
  own sources that starts a process or sends a signal, and of aoide-protocol the
  app calls only `untrusted`. See `crates/shell/README.md`.

The window crate will call the shell. The door is the leaf both sit on, and
it depends on nothing else in this directory.
