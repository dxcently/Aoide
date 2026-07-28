---
type: concept
created: 2026-07-26
updated: 2026-07-26
tags: [aoide, widget, qml, desktop, gadget, theming]
---

# Gadget Dock — the Win7-Sidebar Homage

The `agentWidgets` surface — registered since the walking skeleton but empty —
is now the **gadget dock**: desktop gadgets that realize the user's declared
aesthetic for Aoide, **Windows-7-sidebar-inspired chrome with
ASCII/box-drawing note theming**. It is the desktop's ambient at-a-glance
layer, distinct from the full overlays — and since its v1 redesign it is a
**left-edge pinnable popup**, summoned on demand rather than always visible.

*Grounded in the repo at commit 41be90f; redesigned as the popup at 8f4034e
(flake check + vm-boot green). Implementation:
`modules/facets/quickshell/qml/AoideAgentWidgets.qml` plus the gadget files —
see [[Quickshell]] and [[Codebase]].*

## Posture — a left-edge pinnable popup

The first cut was an always-visible right-edge column. At 8f4034e the dock
became a **drawer**: it rests off-screen past the LEFT edge and slides in
(150 ms OutCubic — the Win7-vibe reveal) on either of two triggers:

- **Hot edge** — a 5 px full-height hover strip on the left screen edge. Pure
  QML, so it works live today even with the stubbed bridge.
- **SUPER+G** — the compositor keybind, whose bridge path is the new stub verb
  `aoide shell dock toggle` (it replaced `aoide shell graph toggle`; see
  [[aoide-cli]]). The keybind path is defined as **open-and-pin**: a keybind
  that merely peeked would auto-hide the instant the pointer settled, which
  would make it useless.

**Pinning.** The header chrome — `╔═[ GADGETS ]══[pin]═╗`, a new idiom beyond
`GadgetFrame`'s title-only bar — carries the pin affordance: `[+]` unpinned,
`[■]` pinned. Pinned, the dock stays put regardless of the pointer.

**Auto-hide with grace.** Unpinned, the dock only retracts after a real
**400 ms grace timer**: the panel's x-position binds to a timer-gated `shown`
property, so leaving the hover union arms the timer and re-entering cancels it
— crossing the small gap between strip and panel can never flap the drawer.
(This was a review fix: the first cut had a decorative timer that gated
nothing.) The hot strip and the panel form **one hover union** via `NoButton`
hover catchers, so hovering rows and clicking gadgets never count as leaving —
clicks fall through to the gadgets.

The state machine is explicit — three states drive the slide:

| State | Condition | Panel |
|---|---|---|
| hidden | not pinned, pointer outside strip + panel | off-screen left |
| peeking | not pinned, pointer in the hover union | shown (hover) |
| pinned | pinned via keybind/bridge or the `[pin]` click | shown (sticky) |

The dock remains **non-exclusive** — it reserves no screen space, sitting over
the wallpaper like the other skeleton surfaces. The v1 note sharpened with the
redesign: when layer-shell typing lands, the dock should claim a
**non-exclusive top/overlay layer** so the hot edge still works above tiled
windows.

One consequence for the graph story: the dock popup is now the **primary DAG
affordance**. The standalone `AoideSessionGraph` overlay keeps **no keybind**
— it is dormant, bridge-only, retained as the seam for a future full-screen
DAG view ([[Session-Graph]]).

## Chrome

Every gadget wears the same reusable frame, **`GadgetFrame.qml`**:

- Box-drawing chrome — `╔═[ TITLE ]═╗` header, `╚═╝` footer — in a monospace
  face, the ASCII half of the aesthetic.
- An Aero-glass body: a translucent note-background fill (about 0.72 opacity)
  sitting over the compositor's Hyprland blur — the Win7 half.

All colour comes from [[Notes|notes]] — zero hardcoded hex anywhere in the
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
  primary DAG affordance now that the overlay is dormant — with
  `├─ └─ │` tree limbs matching the CLI render. It instantiates the shared
  `GraphModel.qml` rather than re-deriving the tree.
- **`ClockGadget`** — a large monospace `HH:MM` in a box frame plus the date,
  on a one-second timer.
- **`MeterGadget`** — CPU and RAM gauges rendered as `[▓▓▓░░░]` bars, fed by
  `/proc/stat` (busy-time delta) and `/proc/meminfo` through `FileView` on a
  two-second reload. Reading files — not spawning processes — keeps it inside
  the no-shell-out-from-QML rule; it is the documented interim until a
  system-telemetry stage file exists (open thread).

## The waybar-homage wave — three more gadgets, and the bar joins the style

The dxflake-parity work (branch `worktree-devtools-dendrites`, commit
`a808c33`, flake check + vm-boot green; pending merge) grew the roster to
seven and pulled `AoideBar` into the same visual language:

- **`NowPlayingGadget`** — Mpris as ASCII: `♪ « artist – title »` marquee,
  `▮▯` progress bar, glyph transport controls gated on the player's `can*`
  flags.
- **`PowerGadget`** — the rest-notation battery icons (`𝄽 𝄾 𝄿 𝅀 𝅁 𝅂`) with a
  `[▓░]` charge bar via UPower, plus a network glyph from `/proc/net/route`
  through `FileView` (this quickshell rev ships no NetworkManager service —
  same files-not-processes rule as `MeterGadget`); shows an honest
  "AC (no battery)" on desktops.
- **`CalendarGadget`** — an ASCII box-drawing month grid, today highlighted
  from the note accent; it doubles as the bar clock's anchored drop-popup
  (the waybar calendar-tooltip's descendant).

The bar itself is now the dxflake waybar homage — slim glass strip, 𝄞 power
cell, musical-notation workspaces with a sliding active box, kaomoji title
rewrite, slash-separated note-glyph cells — enhanced with Quickshell-native
interactivity (scroll/click/hover popouts, mpris marquee + progress, urgent
pulse) and an Aoide-native live-sessions cell riding the `sessions.json` seam
that click-toggles this dock. Visual target preserved at
`references/dxflake-rice-screenshot.png`.

## Design memory

The aesthetic is not just implemented — it is **recorded as a design
decision** in the default rice's liner (`modules/rime/default/liner/intent.md`,
Iteration Log), so future rice generations inherit the Win7-plus-ASCII
intent as accumulated taste per the songbook discipline ([[Self-Ricing]]).

## Why it matters beyond the pixels

The dock is a working proof of the [[Widget-Maker]] thesis at surface scale:
four independent gadgets sharing one frame component and one graph model, all
declaratively themed, all added without touching any other surface. Future
gadgets follow the same recipe — a `GadgetFrame`, notes-only colour, stage
files (or, until then, documented read-only interims) for data.

## Related

- [[Quickshell]]
- [[Terminal-Commander]]
- [[Session-Graph]]
- [[Widget-Maker]]
- [[Notes]]
- [[Self-Ricing]]
- [[shellbridge]]
- [[aoide-cli]]
- [[Codebase]]
