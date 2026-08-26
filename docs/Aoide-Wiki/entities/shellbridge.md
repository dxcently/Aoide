---
type: entity
created: 2026-07-25
updated: 2026-08-25
tags: [aoide, bridge, ipc, desktop]
source: "[[references/AOIDE-HANDOFF]]"
---

# shellbridge

The bridge between the daemon/agents and the live desktop. It speaks two
directions: atomic JSON state files out (read by Quickshell), and
unix-socket commands in (sent by agents or the CLI). It consumes the
Hyprland IPC socket directly. No MCP in QML, ever: all agent-to-shell
communication routes through shellbridge's socket; Quickshell only reads
state files and issues socket commands.

A widget click in Quickshell issues a socket command to shellbridge, which
calls `hyprctl dispatch focuswindow address:…` to bring the target window
forward. Claude Code hook states (`Notification`/`Stop`/pre-post tool use)
post their state through shellbridge, making the connection-state widget in
the bar a live reflection of the agent's execution phase.

## Implementation

shellbridge runs as the `shellbridge` systemd user service via `aoide
shellbridge --run`, a sub-command of the [[aoide-cli]] binary. Its socket
path is a hard contract, never computed independently:
`$XDG_RUNTIME_DIR/aoide/shellbridge.sock` (the unit's `RuntimeDirectory=aoide`
creates the directory). It publishes two atomically-written stage files
besides `livery.json`: `song/stage/sessions.json` (the agent session
roster — `{ sessionId, agent, windowAddress, cwd, state, startedAt }`,
backing the [[Terminal-Commander]] session-jump) and `song/stage/hooks.json`
(live Claude Code hook phases — `{ sessionId, phase, updatedAt }`). The
atomic writer (write-temp-then-rename) seeds both files with their v0
shapes and keeps them current as sessions come and go.

The socket accept loop binds the contract socket and accepts newline-JSON
commands: a `{cmd:"focuswindow", address}` line drives `hyprctl dispatch
focuswindow address:…`, the same session-jump primitive `graph focus` uses
from the CLI door. This is the command the [[Gadget-Dock]]'s terminal-manager
gadget and the [[Terminal-Commander]] roster call on a row click — QML
issues the socket command, never shells out. The command set is narrow (jump
only; prune remains an open thread, see below).

**`hyprctl` must be on the service PATH, or every click fails silently.**
Both the socket handler's `focus_window` and the window→session event
listener shell out to `hyprctl`. A systemd user unit's default PATH is
minimal (coreutils/findutils/grep/sed/systemd) and excludes the compositor,
so a unit-level `path = [ pkgs.hyprland ]` on both the `shellbridge` and
`aoide-graph-reap` services (`modules/nucleus/shellbridge.nix`) puts
`hyprctl` on PATH explicitly. It is a *unit* option, a sibling of
`serviceConfig`, not a `serviceConfig` key — nesting it there emits an inert
raw `path=` line and PATH stays broken. Without it, every widget click
fails with `hyprctl unavailable: No such file or directory` (audited as
`focus-failed`, not logged to the journal): the CLI `graph focus` inherits
the caller's richer PATH and keeps working while the daemon's minimal PATH
breaks silently.

Reads and jumps go through the socket; the write door for session state
remains the CLI. The `aoide graph session` command family upserts those same
stage files atomically (each mutation re-stages `graph.json`, so the read
path lights up immediately). `session start --id <id> [--agent --cwd
--window --parent]` upserts a running `sessions.json` record, idempotent —
a re-start updates the provided fields and never clobbers `startedAt`;
`--parent` is cycle-checked like `graph link`. `session phase --id --phase`
upserts the one-per-session `hooks.json` record. `session end --id` marks
the session (and its hook phase) `done`, an ok no-op on an unknown id.
`session hook` is a stdin door for agent harnesses: it reads one
Claude-Code hook JSON (`session_id`, `cwd`, `hook_event_name`) and maps
SessionStart→start, UserPromptSubmit/PreToolUse→phase running, Stop→phase
waiting, SessionEnd→end, never exiting non-zero so it is safe to wire into
interactive-session hooks. `startedAt` is stamped ISO-8601 UTC (hand-rolled,
round-tripping the conductor reader — no chrono in the offline lock).

The stage also carries two more files: `song/stage/projects.json` (the
project registry, v0 `{schemaVersion, projects: [{name, path}]}`) and
`song/stage/graph.json` (the resolved DAG, v0 `{schemaVersion, nodes, edges:
[{from, to, kind}]}`, emitted by `aoide graph emit` with the same atomic
write). A `sessions.json` record may additionally carry the optional
`parentSessionId` (additive, still v0), the spawned-by edge. `aoide graph
link` and `aoide graph session start --parent` both write that field;
shellbridge stamping it at spawn time over the socket is still an open
thread. The graph stage rewriters round-trip unknown fields, so they never
clobber what shellbridge (or any other writer) adds to a record.

The env-var seam is a documented contract ("Stage-dir resolution",
`CONTRACTS.md §4`): the Rust `stage_dir()` honours `$AOIDE_STAGE_DIR` when
set to an absolute path (the unit sets `%h/Aoide/song/stage`; empty or
relative values are ignored so runtime paths never resolve against an
arbitrary cwd), else it falls back to the `aoide_home()` derivation. A
serialized test pins the precedence; the companion `AOIDE_AUDIT_LOG` seam
follows the same contract. The env-var test mutex in `shellbridge.rs` is
module-local — fine while only this module's tests touch env vars, a
conflict risk if others grow them (open thread).

The [[Gadget-Dock]]'s terminal-manager gadget renders its per-row prune `[x]`
disabled precisely because no prune command exists on the socket yet — QML never
invents IPC. Growing the command set (prune next) is an open thread, as is
stamping `parentSessionId` at spawn time.

**Authoritative window capture.** Alongside the socket accept loop, `lyra
shellbridge --run` spawns a background thread that reads Hyprland's
`socket2` event stream (`$XDG_RUNTIME_DIR/hypr/
$HYPRLAND_INSTANCE_SIGNATURE/.socket2.sock`). On each window lifecycle
event it keeps `sessions.json` authoritative: an `openwindow` (re-checked on
`movewindow`/`movewindowv2`/`windowtitle`) resolves any tracked session
still missing its `windowAddress` by walking that session's recorded `pid`
up the `/proc` ppid chain and matching an ancestor against `hyprctl clients
-j`, then stamping the client's canonical `0x…` address via the atomic
graph writer. A `closewindow` clears that address off whatever session held
it. The lazy hook-time backfill alone leaves the address frequently empty
at click time, so capturing it at window-creation time keeps the
[[Terminal-Commander]] jump reliable. The listener reuses the same
pid-ancestry ↔ clients helpers as launch-time discovery (kept as a
fallback), runs concurrently with the accept loop without blocking it, and
degrades to a logged no-op with no `HYPRLAND_INSTANCE_SIGNATURE`
(headless/non-Hypr aoide is unaffected).

A terminal killed uncatchably (SIGKILL, SUPER+Q) cannot run its own `graph
session end`. The **liveness reaper** (`aoide graph reap`, on a systemd user
timer every ~12s after an initial 15s delay) sweeps the roster shellbridge
publishes and marks such a session dead when its window is gone (per `hyprctl
clients -j`) OR its pid's `/proc` entry is gone — never both required, never
a false reap of a live, signal-less session. See [[Session-Graph]] ("Liveness
reaping") for the predicate.

## Related

- [[Desktop-Architecture]]
- [[Quickshell]]
- [[aoided]]
- [[Hyprland]]
- [[aoide-cli]]
- [[Codebase]]
- [[Terminal-Commander]]
- [[Session-Graph]]
- [[Gadget-Dock]]
- [[Widget-Bridge-Contract]]
