---
type: entity
created: 2026-07-25
updated: 2026-08-25
tags: [aoide, compositor, wayland, hyprland]
source: "[[references/AOIDE-HANDOFF]]"
---

# Hyprland

The Wayland compositor that Aoide runs on. Hyprland is the only *window*
multiplexer in the stack — no tmux/screen-style terminal multiplexer is
part of the design. (A narrower PTY layer does exist for agent control —
`aoide conduct`, one PTY per conducted session, purpose-built for
`graph send` injection, not a general terminal multiplexer — see
[[shellbridge]], [[Agent-Hooking]].) Its facet (`modules/facets/compositor/`)
renders live appearance via `hyprctl`, reading only `aoide.livery` and
`aoide.arrangement` like every other facet (house rule 5).

shellbridge consumes the Hyprland IPC socket to track windows and dispatch focus commands. The session-jump flow (`hyprctl dispatch focuswindow address:…`) depends on this IPC path.

Hyprland is a declared input in Aoide's own flake (`github:hyprwm/Hyprland`), and `yomi-strix` — Aoide's host, templated from [[dxflake]]'s own `yomi-strix` — runs it.

## Related

- [[Desktop-Architecture]]
- [[shellbridge]]
- [[Quickshell]]
- [[Agent-Hooking]]
