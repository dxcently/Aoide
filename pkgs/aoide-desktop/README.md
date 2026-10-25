# aoide-desktop

The desktop app for Aoide: a Tauri 2 window over the orchestration the terminal
already drives. This directory is its own Cargo workspace. It is not a member
of `pkgs/aoide`'s workspace and keeps its own `Cargo.lock`, so GUI crates never
reach the core lock. Its crates are GUI-free libraries; the window is a later
crate.

## Crates

- `crates/door` (`aoide-desktop-door`): the loopback door client. See
  `crates/door/README.md`.
- `crates/shell` (`aoide-desktop-shell`): the one place the app starts a process
  or sends a signal, and the app's own doors and asks. See
  `crates/shell/README.md`.

The window crate will call the shell. The door is the leaf both sit on, and
it depends on nothing else in this directory.
