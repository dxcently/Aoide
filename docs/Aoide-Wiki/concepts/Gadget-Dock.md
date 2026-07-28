---
type: concept
created: 2026-07-26
updated: 2026-07-28
tags: [aoide, widget, qml, desktop, gadget, theming]
---

# Gadget Dock — the Win7-Sidebar Homage

The `agentWidgets` surface is the **gadget dock**: desktop gadgets that
realize the user's declared aesthetic for Aoide, **Windows-7-sidebar-inspired
chrome with ASCII/box-drawing note theming**. It is the desktop's ambient
at-a-glance layer, distinct from the full overlays: a **left-edge pinnable
popup**, summoned on demand.

*Verified via flake check + vm-boot. Implementation:
`modules/facets/quickshell/qml/AoideAgentWidgets.qml` plus the gadget files —
see [[Quickshell]] and [[Codebase]].*

## Posture — a left-edge pinnable popup

The dock is a **drawer**: it rests off-screen past the LEFT edge and slides in
(150 ms OutCubic — the Win7-vibe reveal) on either of two triggers:

- **Hot edge** — a 5 px full-height hover strip on the left screen edge. Pure
  QML, so it works live today even with the stubbed bridge.
- **SUPER+G** — the compositor keybind, whose bridge path is the stub verb
  `aoide shell dock toggle` (see [[aoide-cli]]). The keybind path is defined as
  **open-and-pin**: it opens the dock and pins it, so it stays open regardless
  of pointer position.

**Pinning.** The header chrome — `╔═[ GADGETS ]══[pin]═╗`, a new idiom beyond
`GadgetFrame`'s title-only bar — carries the pin affordance: `[+]` unpinned,
`[■]` pinned. Pinned, the dock stays put regardless of the pointer.

**Auto-hide with grace.** Unpinned, the dock only retracts after a real
**400 ms grace timer**: the panel's x-position binds to a timer-gated `shown`
property, so leaving the hover union arms the timer and re-entering cancels it
— crossing the small gap between strip and panel can never flap the drawer.
The hot strip and the panel form **one hover union** via `NoButton`
hover catchers, so hovering rows and clicking gadgets never count as leaving —
clicks fall through to the gadgets.

The state machine is explicit — three states drive the slide:

| State | Condition | Panel |
|---|---|---|
| hidden | not pinned, pointer outside strip + panel | off-screen left |
| peeking | not pinned, pointer in the hover union | shown (hover) |
| pinned | pinned via keybind/bridge or the `[pin]` click | shown (sticky) |

The dock remains **non-exclusive** — it reserves no screen space, sitting over
the wallpaper like the other skeleton surfaces.

**Status:** specified; no layer-shell typing implemented yet. The design
claims a **non-exclusive top/overlay layer**, so the hot edge works above
tiled windows.

One consequence for the graph story: the dock popup is the **primary DAG
affordance**. The standalone `AoideSessionGraph` overlay keeps **no keybind**
— it is dormant, bridge-only, retained as the seam for a future full-screen
DAG view ([[Session-Graph]]).

## Chrome

Every gadget wears the same reusable frame, **`GadgetFrame.qml`**:

- Box-drawing chrome — `╔═[ TITLE ]═╗` header, `╚═╝` footer — in a monospace
  face, the ASCII half of the aesthetic.
- An Aero-glass body: a translucent drachma-background fill (about 0.72
  opacity) sitting over the compositor's Hyprland blur — the Win7 half.

All colour comes from [[drachma]] — zero hardcoded hex anywhere in the
dock, so the gadgets re-skin with every rice like any other surface.

## The four gadgets

- **`TerminalManagerGadget`** — the [[Terminal-Commander]] roster as a gadget:
  live sessions from `sessions.json` + `hooks.json`, merged
  latest-hook-phase-over-roster-state exactly as the Rust graph module does,
  showing agent, coloured state, shortened cwd, and elapsed time. Clicking a
  row jumps to its terminal via the [[shellbridge]] socket. A per-row prune
  `[x]` affordance is rendered **disabled**: the shellbridge socket protocol
  has no prune verb yet, and QML never invents IPC (open thread).
- **`DagGraphGadget`** — a compact [[Session-Graph]] view — the desktop's
  primary DAG affordance — with `├─ └─ │` tree limbs matching the CLI render.
  It instantiates the shared
  `GraphModel.qml` as the single source of tree state.
- **`ClockGadget`** — a large monospace `HH:MM` in a box frame plus the date,
  on a one-second timer.
- **`MeterGadget`** — CPU and RAM gauges rendered as `[▓▓▓░░░]` bars, fed by
  `/proc/stat` (busy-time delta) and `/proc/meminfo` through `FileView` on a
  two-second reload. Reading files — not spawning processes — keeps it inside
  the no-shell-out-from-QML rule; it is the documented interim: no
  system-telemetry stage file exists yet (open thread).

## The waybar-homage wave — three more gadgets, and the bar joins the style

The roster holds seven gadgets, and `AoideBar` shares the same dxflake-parity
visual language:

- **`NowPlayingGadget`** — Mpris as ASCII: `♪ « artist – title »` marquee,
  `▮▯` progress bar, glyph transport controls gated on the player's `can*`
  flags.
- **`PowerGadget`** — the rest-notation battery icons (`𝄽 𝄾 𝄿 𝅀 𝅁 𝅂`) with a
  `[▓░]` charge bar via UPower, plus a network glyph from `/proc/net/route`
  through `FileView` (this quickshell rev ships no NetworkManager service —
  same files-not-processes rule as `MeterGadget`); shows an honest
  "AC (no battery)" on desktops.
- **`CalendarGadget`** — an ASCII box-drawing month grid, today highlighted
  from the drachma accent; it doubles as the bar clock's anchored drop-popup
  (the waybar calendar-tooltip's descendant).

The bar itself is the dxflake waybar homage — slim glass strip, 𝄞 power
cell, musical-notation workspaces with a sliding active box, kaomoji title
rewrite, slash-separated note-glyph cells — enhanced with Quickshell-native
interactivity (scroll/click/hover popouts, mpris marquee + progress, urgent
pulse) and an Aoide-native live-sessions cell riding the `sessions.json` seam
that click-toggles this dock. Visual target preserved at
`references/dxflake-rice-screenshot.png`.

## Design memory

The aesthetic is not just implemented — it is **recorded as a design
decision** in the default song's design memory
(`song/songbook/default/design/intent.md`, Iteration Log), so future rice
generations inherit the Win7-plus-ASCII intent as accumulated taste per the
songbook discipline ([[Self-Ricing]]).

## Why it matters beyond the pixels

The dock is a working proof of the [[Widget-Maker]] thesis at surface scale:
seven independent gadgets sharing one frame component and one graph model, all
declaratively themed, all added without touching any other surface. Future
gadgets follow the same recipe — a `GadgetFrame`, drachma-only colour, and
stage files (or a documented read-only interim) for data.

## Related

- [[Quickshell]]
- [[Terminal-Commander]]
- [[Session-Graph]]
- [[Widget-Maker]]
- [[drachma]]
- [[Self-Ricing]]
- [[shellbridge]]
- [[aoide-cli]]
- [[Codebase]]
