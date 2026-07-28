---
type: concept
created: 2026-07-26
updated: 2026-07-26
tags: [aoide, widget, terminal, agent, session]
source: "[[references/AOIDE-HANDOFF]]"
---

# Terminal Commander — the Agent-Session Widget

A flagship shipped widget (conductor-class): a live roster of the terminals running
agents, so you always know what is running where — and can **jump to any of them
by click or keybind**. Agents spawn terminals faster than a human tracks them;
the terminal commander is the single pane that herds them. It is also the cleanest
exemplar of the [[Widget-Maker]] thesis — a watcher + a widget + a keybind, all
declarative and themed by [[Notes|notes]].

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

- The conduct wrapper **registers each session together with its Hyprland window
  address** at launch — now actually populated by **phase ② discovery**: `conduct`
  walks its own pid up the `/proc` ppid chain and matches an ancestor pid against
  `hyprctl clients -j`, taking the first matching client's `address` (best-effort;
  a missing Hyprland or no match just leaves it empty). This is the authoritative
  "which window is which agent" source `graph focus` jumps by.
- Claude Code hooks (`Notification` / `Stop` / `Pre-PostToolUse`) post execution
  phase, so each row shows live state (running · awaiting input · idle · done).
  The **hook door discovers windows too**: the same pid-ancestry ↔ `hyprctl
  clients -j` match runs at `SessionStart` (and is backfilled on any later hook
  while still empty), so a hook-registered Claude Code session — not just a
  `conduct`-wrapped one — becomes focus-jumpable without ever wrapping its
  process.
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

The roster also has its **desktop gadget** now (commit 41be90f): the
`TerminalManagerGadget` in the [[Gadget-Dock]] renders it live from
`sessions.json` + `hooks.json`, merging the latest hook phase over the raw
roster state exactly as the Rust graph module does — agent, coloured state,
shortened cwd, elapsed; click a row to jump. Its per-row prune `[x]` is
rendered disabled until the shellbridge socket grows a prune verb (open
thread) — the dock never invents IPC.

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
