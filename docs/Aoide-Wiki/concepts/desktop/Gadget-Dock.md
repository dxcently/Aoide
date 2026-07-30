---
type: concept
created: 2026-07-26
updated: 2026-07-30
tags: [aoide, widget, qml, desktop, gadget]
---

# Gadget Dock

The **gadget dock** (`agentWidgets`) is the desktop's at-a-glance conductor
surface: a left-edge pinnable drawer holding Aoide's orchestration gadgets,
summoned on demand and distinct from the full-screen overlays.

*Implementation: `modules/facets/quickshell/qml/AoideAgentWidgets.qml` plus the
gadget files — see [[Quickshell]] and [[Codebase]]. Appearance is not specified
here: the dock re-skins with the active song, and rice design memory lives in
the songbook ([[Self-Ricing]]), not on this page.*

## What the dock holds

For Aoide, the dock carries the two orchestration gadgets:

- **Conductor control** — a mini [[Conductor-Channel|conductor]] view: the live
  agent-session roster with per-session state, the at-a-glance control surface
  for the sessions Aoide is running.
- **Terminal sessions** — the [[Terminal-Commander]] roster: live sessions from
  `sessions.json` + `hooks.json` (merged latest-hook-phase-over-roster-state as
  the Rust graph module does), showing agent, state, shortened cwd, and elapsed
  time. Clicking a row jumps to its terminal via the [[shellbridge]] socket.

The full session DAG is **not** a dock gadget — the standalone
`AoideSessionGraph` overlay is the seam retained for a future full-screen
session-graph view ([[Session-Graph]]).

Nothing else is part of the canonical dock. A fork can add its own gadgets on
the same recipe (a frame, drachma-only colour, a stage file for data).

## Posture — a left-edge pinnable drawer

The dock rests off-screen past the LEFT edge and slides in on either trigger:

- **Hot edge** — a full-height hover strip on the left screen edge.
- **SUPER+G** — the compositor keybind, defined open-and-pin: it opens the dock
  and pins it, so it stays open regardless of pointer position.

**Pinning.** The header carries a pin affordance; pinned, the dock stays put
regardless of the pointer. **Auto-hide with grace.** Unpinned, it retracts only
after a ~400 ms grace timer; the hot strip and panel form one hover union, so
hovering rows and clicking gadgets never count as leaving. The dock is
**non-exclusive** — it reserves no screen space.

## Related

- [[Quickshell]]
- [[Terminal-Commander]]
- [[Conductor-Channel]]
- [[Session-Graph]]
- [[Widget-Maker]]
- [[shellbridge]]
- [[Codebase]]
