---
type: concept
created: 2026-07-26
updated: 2026-07-30
tags: [aoide, widget, qml, desktop, gadget]
---

# Gadget Dock

The **gadget dock** (`agentWidgets`) is the desktop's at-a-glance conductor
surface: a left-edge panel holding Aoide's orchestration/system gadgets,
summoned on demand and distinct from the terminal/TUI DAG views.

*Implementation: `modules/facets/quickshell/qml/AoidePanel.qml` plus the
gadget files — see [[Quickshell]] and [[Codebase]]. Appearance is not specified
here: the dock re-skins with the active song, and rice design memory lives in
the songbook ([[Self-Ricing]]), not on this page.*

## What the dock holds

Four core self-framed gadgets stack in one scrolling column, with an opt-in
fifth (Usage) that draws only when its stage file exists:

- **Conductor** (`ConductorGadget.qml`) — the agent-session roster as a tree:
  agent/sub-agent sessions plus any shell an agent is attached to (a shared
  `windowAddress`); a bare, unattended shell is hidden. Rows are named by
  session `title`, coloured/glyphed by the canonical `state`
  (♪ working · 𝄐 awaiting · 𝄁 stopped · 𝄽 idle · 𝄂 done), and show each agent's `say`;
  sub-agents beam beneath their parent one indent level
  ([[Widget-Bridge-Contract]]). Each row also carries a plain arabic-numeral
  `wsN` tag next to its state label, naming the row's live Hyprland
  `workspace` — deliberately a bare number, distinct from the bar's own
  note-glyph workspace vocabulary ([[Terminal-Commander]]). Three further marks
  ride each row and its grouping:
  - **Project grouping.** Sessions group under **collapsible project headers**,
    anchored by the longest matching project root (the same anchoring
    `graph/model.rs` uses). A header folds its bucket away but keeps showing its
    **rollup** — a max-fill percentage and a token sum over the bucket — so a
    collapsed, near-full project keeps flagging itself; a fleet-wide rollup over
    every session sits at the top regardless of fold state.
  - **Context-window meter.** Each session with an assistant turn shows a
    per-session **context-window meter** — an ASCII `[▓░]` gauge plus a compact
    token count and percentage, in the dock's shared gauge grammar — swapping to
    the urgent role past ~85% fill. A session with no turn yet takes zero
    footprint.
  - **Sudo lock badge.** A conducted shell blocked at a `sudo` **password**
    prompt raises the `needsSudo` signal, which the row renders as a distinct
    monochrome nerd-font **lock badge** — deliberately different from ordinary
    `awaiting` ("the agent wants a permission answer"): this reads as "it's your
    terminal password". This is exactly the A2A door's `AUTH_REQUIRED` task
    state ([[A2A-Door]]).
- **Terminals** (`TerminalsGadget.qml`) — the [[Terminal-Commander]] roster:
  every live terminal window, tracked or not — the daemon publishes a synthetic
  record for each untracked one, so the widget reads `sessions.json` alone and
  filters/de-dupes by window address. A row's headline is `activity` — the
  foreground command or edited file, or the bare shell/agent process when idle
  — with `cwd` as subtext, and the same plain `wsN` workspace tag as Conductor.
- **Meters** (`MetersGadget.qml`) — CPU/RAM read directly from `/proc/stat` /
  `/proc/meminfo` on a ~2s tick; still the documented interim until a
  system-telemetry stage file exists (open thread).
- **Power** (`PowerVitalsGadget.qml`) — battery via UPower and the network
  link via `/proc/net/route`.
- **Usage** (`UsageGadget.qml`) — the opt-in claude.ai usage stele
  (`aoide.usage.enable`, off by default). It reads `state/usage.json` — the
  local this-machine token/cost estimate the `aoide usage` poller writes,
  over the account's (unofficial) claude.ai rolling window and weekly caps —
  and is **content-driven**: it draws only once that file exists, so a host
  without the poller enabled shows nothing.

Clicking a Conductor/Terminals row jumps to its terminal via the
[[shellbridge]] socket. The full session DAG has no standalone desktop
surface today — `aoide graph view`/`--json` and the `aoide conductor` TUI are its
renderers; the dock's Conductor gadget gives the desktop its at-a-glance
agent-tree view instead of a literal graph diagram ([[Session-Graph]]).

Nothing else is part of the canonical dock — the bar's own calendar popout is
not a dock gadget; its body is resolved per-song by the staging engine
([[Widget-Maker#The staging engine — a song overrides desktop chrome]]), not
hosted here. A fork can add its own gadgets on the same recipe (a frame,
drachma-only colour, a stage file for data).

## Posture — a left-edge panel

The dock rests off-screen past the LEFT edge with its fore-edge always
peeking, and slides fully in on either trigger:

- **Hot edge** — a 6px hover strip on the left screen edge.
- **SUPER+G** — an in-process Hyprland global shortcut (`aoide:dock`) the
  panel registers itself, not a CLI verb: it pins the dock open, and a second
  press dismisses it.

**Pinning.** Pinned, the dock stays put regardless of the pointer.
**Auto-hide with grace.** Unpinned, it retracts after a ~450 ms grace timer
once the hover union (hot strip + panel) is left. **Awaiting alert.** The
fore-edge also peeks out on its own, unprompted, whenever any session is
`awaiting` and that alert hasn't yet been acknowledged (opening the dock any
way acknowledges it). The dock is **non-exclusive** — it reserves no screen
space.

## Related

- [[Quickshell]]
- [[Terminal-Commander]]
- [[Conductor-Channel]]
- [[Session-Graph]]
- [[Widget-Maker]]
- [[shellbridge]]
- [[Codebase]]
- [[Widget-Bridge-Contract]]
- [[A2A-Door]]
