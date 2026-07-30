---
type: concept
created: 2026-07-30
tags: [aoide, bridge, desktop, widget, quickshell, session, ipc]
---

# Widget–Bridge Contract — widgets are pure views of the bridge

The desktop widgets ([[Quickshell]]) render **only** what the bridge publishes.
The bridge — [[shellbridge]] plus the Claude Code hook door and the `conduct`
PTY — is the single source of truth for every agent/shell fact; a widget is a
pure view over the stage files and issues only a narrow set of socket commands
back. A widget never derives state, never enumerates the system, never invents
IPC. This page is the contract both sides are built to.

## The bridges (who writes the truth)

- **[[shellbridge]]** — the daemon. Atomic JSON stage files out (`song/stage/`),
  a unix socket in, and the Hyprland `socket2` window listener that keeps each
  session's `windowAddress`/`workspace` authoritative.
- **The hook door** (`aoide graph session hook`) — a short-lived process per
  Claude Code hook event ([[Agent-Hooking]]) that maps the event to a canonical
  session state (below). This is the Claude↔desktop bridge.
- **`conduct`** ([[Conductor-Channel]]) — owns every terminal's PTY, so it is the
  producer of a shell's live `cwd`, current command, and idle/working state, and
  the injection channel for `graph send`.
- **`ShellBridge.qml`** — QML's ONLY outbound channel: newline-JSON socket
  commands. No MCP, no HTTP, no shell-exec from QML.

## What the bridge publishes — `sessions.json` is the widget contract

`song/stage/sessions.json` is the ONE file the roster widgets watch. Each record:

| field | meaning |
|---|---|
| `sessionId` | key. Sub-agent nodes are `sub:<tool_use_id>`. |
| `kind` | `agent` \| `shell` \| `subagent` — what the record IS (widgets never infer role from the agent string). |
| `state` | the CANONICAL live state — exactly one of `working` \| `awaiting` \| `idle` \| `done`. |
| `activity` | the current command (a shell's foreground command) or tool/sub-task (an agent); absent when nothing runs. |
| `title` | the human session NAME — the first user prompt (set-once), or a `graph send` steer. |
| `cwd` | live working directory (a conducted shell's follows `cd`). |
| `parentSessionId` | the tree edge — a sub-agent's owner, or a nested claude's launcher. |
| `agent`, `windowAddress`, `workspace`, `pid`, `conductable`, `socket` | identity + jump/lifecycle. |

**Canonical state vocabulary** — the daemon emits these verbatim; the widget
switches on the string, it does NOT regex-guess:
- `working` — in a turn / running a tool, or a shell running a foreground command.
- `awaiting` — needs the user. Set ONLY by the `Notification` hook (a permission
  prompt, or the ~60 s idle-input prompt). The invariant is **`needsInput ⇔
  state == awaiting`**: a finished turn (`Stop`) settles to `idle`, not
  `awaiting`, so the anxious state keeps its signal value.
- `idle` — alive but at rest (a fresh session, a finished turn, a bare prompt).
- `done` — ended.

`hooks.json`/`graph.json` are the audit + derived-DAG mirrors; a widget need not
read them. Legacy vocabulary (`running`/`waiting`/`blocked`) is folded to the
canonical set by a read-side shim (`canonical_state`) the first time any writer
touches the file, so migration needs no rollout.

## The state machine (hook → canonical state)

`SessionStart`→`idle` · `UserPromptSubmit`→`working` (and the prompt NAMES the
session, set-once) · `PreToolUse`/`PostToolUse`→`working`, setting the owner's
`activity` to the tool · `Stop`→`idle` · `Notification`(permission_prompt /
idle_prompt)→`awaiting` · `SessionEnd`→`done`. The idle-prompt only becomes
`awaiting` when the turn is still `working` (an unanswered question), never for a
settled session.

## The sub-agent tree

A `Task` a claude agent spawns becomes a `subagent` node keyed `sub:<tool_use_id>`,
named from the Task description, parented to its owner — the session, or the
parent `sub:` node when a sub-agent spawns another (so nesting does not flatten
onto the root). A tool run inside a sub-agent routes its `activity` to that
sub-node, never the parent. The node is removed when its `Task` returns
(`PostToolUse`/`SubagentStop`), and a `SessionEnd` cascades to remove the whole
subtree (Task nodes carry no pid/window, so the liveness reaper cannot). A nested
real `claude` links via the `AOIDE_SESSION_ID` it inherits in the hook env — so a
claude conducting another claude nests too.

The [[Gadget-Dock|Conductor]] renders this hierarchy as musical **beaming** —
children are smaller notes joined to the parent by a horizontal beam, one indent
level — staying inside the stave motif rather than a directory tree. Only
agent→agent/subagent nesting shows in the Conductor; shells stay in the Terminals
temple.

## The jump — resolved in the bridge

A roster row-click sends `{cmd:"focussession", sessionId}`; the DAEMON resolves
the id to the session's `windowAddress` (and focuses it, which also brings its
workspace forward), or falls back to `hyprctl dispatch workspace` when the address
is not resolved yet. The widget sends only the sessionId it already holds — never
a stale or empty address. (The bare `focuswindow` verb remains for a window with
no session id.)

## The rules a widget is built by

1. **Pure view.** Render the published fields. Never derive `state` (no regex on
   a state string), never enumerate the system (no `hyprctl`/`/proc`/MCP in QML),
   never invent IPC.
2. **Single source of truth.** If a widget needs a fact, the BRIDGE must publish
   it — extend the schema, not the widget.
3. **One canonical file.** Watch `sessions.json`; treat `hooks.json`/`graph.json`
   as audit/derived.
4. **Outbound is a narrow socket.** `focussession`/`focuswindow` jumps and
   `graph send` injection — nothing else leaves QML.
5. **Colour only from [[drachma]]**; hard corners; the music-glyph state contract
   (♪ working · 𝄐 awaiting · 𝄽 idle · 𝄂 done) is a hard contract.
6. **Degrade.** An empty/missing stage file is an empty roster; off-Hyprland the
   jump/enumeration simply no-ops.
7. **Hot-reload discipline.** `sessions.json` heartbeats constantly, so reassign a
   model array only when the RENDERED roster actually changed (else the `ListView`
   tears down delegates), and key hover on the row's `sessionId` so a refresh can
   never drop it mid-hover.

## Rebuild-transparency (no nix hack)

The tracking is undisturbed by a `nixos switch` by construction, not by a service
flag: state lives in files under `$HOME`, its producers are per-hook (short-lived
processes) and per-terminal (`conduct`, which a switch does not kill), and
`seed_if_absent` preserves the roster across the seconds-long shellbridge restart.
Concurrent writers are serialised by an `flock`'d stage lock (atomic rename stops
torn reads; the lock stops lost updates).

**Status:** the bridge/schema half of this contract landed in the Phase 0–3
commits (canonical state, live cwd/command, `activity`/`kind`/`title`, the
sub-agent tree, the `focussession` verb, the `flock` lock, the wired tool/
notification hooks) and runs after the next gated `switch` ([[Rebuild-Gate]]). The
widgets are being brought into conformance (the pure-view switch, the beamed tree,
the `awaiting`-driven dock peek) in the same pass.

## Related

- [[shellbridge]] · [[Quickshell]] · [[Conductor-Channel]] · [[Terminal-Commander]]
- [[Session-Graph]] · [[Agent-Hooking]] · [[Gadget-Dock]] · [[Widget-Maker]]
