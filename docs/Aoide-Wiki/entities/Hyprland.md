---
type: entity
created: 2026-07-25
tags: [aoide, compositor, wayland, hyprland]
source: "[[references/AOIDE-HANDOFF]]"
---

# Hyprland

The Wayland compositor that Aoide runs on. Within the Aoide stack, Hyprland is the only multiplexer — there is no PTY layer. Its facet (`modules/facets/` in the fork) renders live appearance via `hyprctl`, consuming notes like every other facet and touching nothing else.

shellbridge consumes the Hyprland IPC socket to track windows and dispatch focus commands. The session-jump flow (`hyprctl dispatch focuswindow address:…`) depends on this IPC path.

Hyprland is a declared input in the dxflake flake (github.com/hyprwm/Hyprland), and yomi-strix runs it as a dxflake host.

## Related

- [[Desktop-Architecture]]
- [[shellbridge]]
- [[Quickshell]]
