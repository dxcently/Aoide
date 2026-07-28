---
type: concept
created: 2026-07-26
updated: 2026-07-28
tags: [aoide, widget, terminal, agent, session]
source: "[[references/AOIDE-HANDOFF]]"
---

# Terminal Commander — the Agent-Session Widget

A flagship shipped widget (conductor-class): a live roster of the terminals running
agents, so you always know what is running where — and can **jump to any of them
by click or keybind**. Agents spawn terminals faster than a human tracks them;
the terminal commander is the single pane that herds them. It is also the cleanest
exemplar of the [[Widget-Maker]] thesis — a watcher + a widget + a keybind, all
declarative and themed by [[drachma]].

The herdr multiplexer is the prior-art pattern (an external tool — agent-terminal
herding; not part of the Aoide vocabulary, whose word for this duty is
conductor-class — see [[Lexicon]]); Aoide ships this as a first-class widget
rather than a bolt-on, and the agent can regenerate or extend it like any other
integration. The TUI sibling is [[Lexicon|the baton]] (`aoide baton`).

## What it watches

**Every terminal, by default — plus** any agent from any source. As of the
conduct-by-default landing ([[Conductor-Channel]]), the spawn wrapper is no longer
an opt-in first-class path for a few agents: **kitty points its shell at the
`aoide-shell` wrapper, so EVERY window runs its login shell under `aoide conduct`
and self-registers as a tracked, conductable session.** The roster is therefore
the whole desktop's terminals, not just the ones someone remembered to wrap. On
top of that baseline:

- **[[Melete]]-spawned** sessions — the sandboxed agent a `code` run drops into a
  repo checkout, when it surfaces as a local terminal.
- **claude-CLI / other agents** started inside a conducted shell — they inherit
  `AOIDE_SESSION_ID` and nest under it in the DAG (or run `aoide conduct -- <cmd>`
  explicitly for their own conductable node).
- **Discovery fallback** for anything unwrapped (a terminal whose foreground
  process is a known agent CLI).

The watcher is a dendrite ([[Snowflake-Anatomy]]) on the [[aoided]] event stream;
it never polls in QML.

## How it knows which window is which

The plumbing already exists ([[Desktop-Architecture]], [[shellbridge]]):

- **Authoritative, event-driven capture (primary).** [[shellbridge]] runs a
  **Hyprland event listener** on a background thread — it reads the compositor's
  `socket2` event stream (`$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/.socket2.sock`)
  and, the moment a window opens (or moves / retitles), (re)resolves any tracked
  session still missing its `windowAddress`: it walks each session's recorded
  lifecycle **pid** up the `/proc` ppid chain and matches an ancestor against
  `hyprctl clients -j`, stamping the client's canonical `0x…` address into
  `sessions.json` via the atomic graph writer. The same pass also reads the
  client's `workspace.id` and stamps it onto the session's additive **`workspace`**
  field — and, because a resolved window can be *dragged* to another workspace,
  the pass re-stamps every already-resolved session's `workspace` on each window
  event (the `movewindow` / `movewindowv2` re-check), so the id stays current. It
  only ever writes a *present* workspace id (a window momentarily absent from the
  clients list keeps its last-known one — never cleared to a wrong value), and
  degrades gracefully off-Hyprland (no id → the field stays absent, never a
  panic). On `closewindow` it clears the address off whatever session stored it
  (the [[Session-Graph|reaper]] then removes the record via its pid signal).
  **Why this exists:** the address used
  to be filled only *lazily* on the next hook fire, so at click time it was
  frequently empty and the jump failed. Capturing it at window-creation time
  (and re-checking on every window event) makes "which window is which agent"
  reliable — a click always has a live address to jump to. The listener is
  best-effort and off-Hyprland-safe: with no instance signature it logs once and
  disables itself; the shellbridge socket keeps serving regardless.
- **Discovery + hook backfill (fallback).** The older, in-process paths remain as
  belt-and-suspenders: `conduct`'s launch-time **phase ② discovery** (it walks its
  own pid ↔ `hyprctl clients -j`) and the **hook door** — the same pid-ancestry
  match at `SessionStart`, backfilled on any later hook while still empty. These
  cover a session whose window the event listener didn't stamp (e.g. a hook-only
  Claude Code session with no recorded conduct pid for the listener to walk).
- Claude Code hooks (`Notification` / `Stop` / `Pre-PostToolUse`) post execution
  phase, so each row shows live state (running · awaiting input · idle · done).
- Unwrapped agents fall back to process-signal / window-title heuristics.

## The list

[[shellbridge]] writes the roster atomically to `song/stage/*.json`; the
[[Quickshell]] widget reads it (no agent protocol in QML, ever). One row per
session:

| Field | Example |
|---|---|
| agent | claude · melete-run · … |
| where | repo / cwd |
| state | running · awaiting-input · idle · done |
| elapsed | 04:12 |
| window | Hyprland address (used for the jump) |

An `awaiting-input` row can also raise a chime or fire the notification →
messaging bridge ([[Feature-Set]]), so a stalled agent reaches you off-screen.

Conduct-by-default means every closed or killed terminal is also a roster row
to clean up — a `SUPER+Q` or SIGKILL tears the wrap process down uncatchably,
so it can never mark itself `done`. The [[Session-Graph]]'s **liveness
reaper** (`aoide graph reap`, a ~12s systemd timer) sweeps these out-of-band
by window-gone-or-pid-gone, so the roster never accumulates dead rows.

The flat roster now has a **graph layer** on top: the [[Session-Graph]] — a
DAG of projects and sessions (which project anchors each session, which
session spawned which), viewed and managed through the `aoide graph` command
group. The roster is the rows; the graph is the tree they hang from.

The roster also has its **desktop gadget**: the
`TerminalManagerGadget` in the [[Gadget-Dock]] renders it live from
`sessions.json` + `hooks.json`, merging the latest hook phase over the raw
roster state exactly as the Rust graph module does — agent, coloured state,
shortened cwd, elapsed; click a row to jump. Its per-row prune `[x]` is
rendered disabled until the shellbridge socket grows a prune verb (open
thread) — the dock never invents IPC.

### Hover-preview → the bar's workspace glyph

Hovering a roster row now also **previews which workspace that terminal lives
on**, on the bar's [[Gadget-Dock|WorkspaceRow]] (the musical-note-glyph
workspaces). The bridge is deliberately **pure data, not a compositor action**:
the roster already carries each session's `workspace` id (stamped by the event
listener above), so the widget need only *tell the bar which workspace to
preview* — no `hyprctl`, no invented IPC, both surfaces just read shared state.

- The dock row's `HoverHandler` writes the hovered session's `workspace` id to
  `shared.hoveredWorkspace` (a `property int` on shell.qml's shared QtObject,
  the same object that carries `tracedSessionId` for the DAG trace). It threads
  shell → dock → gadget for the *writer* and shell → bar → WorkspaceRow for the
  *reader* — mirroring how `notes`/`bridge` are passed. Hover and click coexist
  (the HoverHandler never steals the click-to-jump `MouseArea`; the dock's
  panel-wide hover union keeps the drawer open through it). On hover-exit the
  value clears to the sentinel `-1`.
- `WorkspaceRow` reads `shared.hoveredWorkspace` and paints a **distinct preview
  highlight** on the matching glyph — a hollow **holoBlue ring** (plus a holoBlue
  tint on the note), deliberately a *different kind* of mark from the true active
  workspace's warm-accent swell + filled pill. Both can show at once: if the
  hovered terminal is on the active workspace, the ring simply frames the accent
  pill. All colour comes from [[drachma]] (`aoide.drachma` roles) — no
  hardcoded hex.
- If a session's window/workspace is not yet resolved (or it is off-screen), its
  `workspace` is `-1` and hovering the row simply highlights nothing — no error.

## Jump — click or keybind

Two ways to reach a terminal, both one hop:

```
 click a row  ─►  shellbridge unix socket  ─►  hyprctl dispatch focuswindow address:…
 keybind      ─►  focus next/prev agent terminal  (or open the roster for quick-select)
```

- **Click** reuses the existing session-jump flow — the correct window in one
  hop, no polling or secondary lookup.
- **Keybind** is a [[Hyprland]] compositor bind (declared in the compositor
  facet): cycle through agent terminals, or pop the roster for type-to-focus.
  Because binds are declarative nix, the keys ship with the rice and are
  rebindable like any other.

## Why it belongs to the widget maker

It is built the way every integration is ([[Widget-Maker]]): a **dendrite**
(watcher + shellbridge wiring), a **Quickshell widget** (the roster), and a
**compositor keybind** — reproducible, note-themed, and removable by one flag.
The agent can extend it (add columns, filters, per-agent actions) on request.

## Related

- [[Session-Graph]]
- [[Gadget-Dock]]
- [[Widget-Maker]]
- [[Desktop-Architecture]]
- [[shellbridge]]
- [[Quickshell]]
- [[Hyprland]]
- [[Melete]]
- [[Feature-Set]]
- [[drachma]]
