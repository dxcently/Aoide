---
type: concept
created: 2026-07-30
updated: 2026-07-30
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
| `state` | the CANONICAL live state — exactly one of `working` \| `awaiting` \| `stopped` \| `idle` \| `done`. |
| `activity` | the current PROCESS: a shell's foreground command / file being edited (`nvim notes.md`, `cargo test`), or its bare shell process when idle (`bash`); an agent's current tool. Absent only when truly nothing runs. |
| `say` | the agent's latest WORDS — the last line of prose it wrote, tail-read from its own Claude Code transcript. Distinct from `activity` (the process); absent for shells and for an agent that hasn't spoken. |
| `title` | the human session NAME. Set-once — whichever source lands FIRST wins: the first user prompt (clipped one-liner), Claude Code's own session title (`custom-title` in the transcript, e.g. "Aoide Dev"), or a `graph send` steer. |
| `cwd` | live working directory (a conducted shell's follows `cd`). |
| `parentSessionId` | the tree edge — a sub-agent's owner, or a nested claude's launcher. |
| `agent`, `windowAddress`, `workspace`, `pid`, `conductable`, `socket` | identity + jump/lifecycle. |

**Canonical state vocabulary** — the daemon emits these verbatim; the widget
switches on the string, it does NOT regex-guess:
- `working` — in a turn / running a tool, or a shell running a foreground command.
- `awaiting` — needs the user. Set ONLY by the `Notification` hook (a permission
  prompt, or the ~60 s idle-input prompt). The invariant is **`needsInput ⇔
  state == awaiting`**: a finished turn (`Stop`) settles to `stopped`, not
  `awaiting`, so the anxious state keeps its signal value.
- `stopped` — the TURN ended and the agent is sitting at its prompt, RECENTLY
  (within the last hour). Alive and warm: the face of a session you just finished
  talking to. Set by `Stop`.
- `idle` — at rest and COLD: `stopped` for more than an hour, or freshly created /
  resumed and not yet active (a shell at its bare prompt reads `idle` too).
- `done` — the SESSION ended (`SessionEnd`, or a reap). Never `stopped`: `Stop`
  ends a turn, `SessionEnd` ends the session, and the two never collapse.

**The `stopped` → `idle` decay.** No hook event fires for a session that is simply
left alone, so the only way out of `stopped` is AGE. The reaper's ~12 s pass
(`aoide graph reap`) ages every `stopped` session an hour after the `Stop` that set
it; the stop instant is the session's rolling `hooks.json` `updatedAt`, so no new
field and no migration are involved. The decay writes `sessions.json` and
`hooks.json` together — the merge overlays the hook phase onto the roster state, so
touching only one file would resurrect the warm badge on the next read.

**Activity labels are basename-clean.** A shell's `activity` label always
collapses `argv[0]` to its basename before display, never a raw path — a known
editor shows as `"<editor> <file>"` (`friendly_editor_command`), and every other
command runs through the same collapse (`generic_command_label`): many
NixOS-wrapped binaries re-exec with `argv[0]` set to their full
`/nix/store/<hash>-<name>/bin/<name>` path (confirmed live: `yazi`), and the
generic label strips that to the bare command name (`yazi`, `rg pattern file.rs`)
while leaving every other argument untouched, falling back to `comm` only when
`argv` is empty.

`hooks.json`/`graph.json` are the audit + derived-DAG mirrors; a widget need not
read them. Legacy vocabulary (`running`/`waiting`/`blocked`) is folded to the
canonical set by a read-side shim (`canonical_state`) the first time any writer
touches the file, so migration needs no rollout.

## The state machine (hook → canonical state)

`SessionStart`→`idle` · `UserPromptSubmit`→`working` (and the prompt NAMES the
session, set-once) · `PreToolUse`/`PostToolUse`→`working`, setting the owner's
`activity` to the tool · `Stop`→`stopped` · `Notification`(permission_prompt /
idle_prompt)→`awaiting` · `SessionEnd`→`done` · and, off the clock rather than off
an event, `stopped`→`idle` after an hour in the reaper pass. The idle-prompt only
becomes `awaiting` when the turn is still `working` (an unanswered question), never
for a settled session. `SessionStart` on an existing id (a resume/compact) folds a
`stopped` record back to `idle` — resumed and not yet active — while leaving a live
`working`/`awaiting` turn alone.

Alongside the state mapping, `Stop`/`PostToolUse`/`UserPromptSubmit`/`Notification`
also tail-read the session's own JSONL transcript (the path a hook payload's
`transcript_path` gives directly, or derived from `session_id` + `cwd` — Claude
Code lays transcripts out at `~/.claude/projects/<munge(cwd)>/<session_id>.jsonl`,
`/` and `.` folded to `-`) to refresh `say` and, set-once, `title` from its
`custom-title` record. A background **Task** sub-agent gets the same treatment
from its OWN dedicated transcript (`<session_id>/subagents/agent-<agent_id>.jsonl`,
correlated to its `sub:<tool_use_id>` node via the sibling `.meta.json`'s
`toolUseId`) — refreshed on every one of the PARENT's hooks, since a background
Task outlives the turn that spawned it.

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
temple. A beamed child row shows its OWN `say` (that sub-agent's latest words).

## One agent per window (dedup)

A terminal window hosts one foreground claude, but Claude Code mints a NEW
`session_id` on compact/resume — and the old record, whose lifecycle-owning `pid`
resolves to the still-alive TERMINAL (not claude itself), never runs its own
`SessionEnd`. Left alone it lingers `working` forever: two "claude" rows for one
terminal, the stale one even masking the real record's `say` in the Terminals
window-merge. The bridge collapses same-window agent records to one:

- **At registration** — a new agent `SessionStart` on a window already held by
  another live agent evicts the sibling immediately (a window address cannot be
  shared by two simultaneously-open windows, so the pair is always a stale re-id).
- **In the reaper** (`aoide graph reap`, the safety net for a window that resolves
  later) — among same-window agents past a short grace, keep the one with a real
  on-disk transcript (→ has `say` → classified `agent` → newest), retire the rest.

Shells are exempt from the rule: a conducted shell and the claude inside it
legitimately share one window, so dedup runs among `agent`-kind records only.

## The two roster temples

Both are pure views of the same `sessions.json`, but they read it differently:

- **[[Gadget-Dock|Conductor]]** — the AGENT tree. Names each row by its session
  `title` ("Aoide Dev"), shows `activity` (the tool) and `say` (the words), and
  beams sub-agents beneath. A conducted shell that only hosts a claude does NOT
  get its own row — the claude represents that terminal (one row per terminal).
- **[[Terminal-Commander|Terminals]]** — the PROCESS view. Each row's headline is
  what the terminal is running (`activity`: the foreground command / edited file,
  or the shell/agent process when idle), with the `cwd` as subtext.

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
   (♪ working · 𝄐 awaiting · 𝄁 stopped · 𝄽 idle · 𝄂 done) is a hard contract.
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

**Status:** live on yomi-strix. The bridge/schema half (canonical state, live
cwd/command, `activity`/`kind`/`title`, the sub-agent tree, the `focussession`
verb, the `flock` lock, the wired tool/notification hooks) and the widget
pure-view conformance (the beamed tree, the `awaiting`-driven dock peek) landed in
the Phase 0–3 pass; a follow-up added `say` (agent + Task words from the
transcript), the `custom-title` session name, the same-window agent dedup
(registration + reaper), the Conductor-as-agent-tree / Terminals-as-process-view
split, and the reaper's own `crate::reap` module — all switched in and verified.

## Related

- [[shellbridge]] · [[Quickshell]] · [[Conductor-Channel]] · [[Terminal-Commander]]
- [[Session-Graph]] · [[Agent-Hooking]] · [[Gadget-Dock]] · [[Widget-Maker]]
