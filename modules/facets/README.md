# modules/facets

The rendering half of the split CONTRACTS.md §0's paint test enforces:
facets carry agnostic bridges and APIs, never song-specific aesthetic
decisions (that's `song/songbook/*/widgets/`'s job). Three facets today:
`compositor` (Hyprland), `quickshell` (the Quickshell shell surface),
`stylix` (baked theming fan-out).

## Named seams (what it exposes)

- `compositor/` — wires Hyprland as the Wayland compositor; applies
  compositor-side livery (gaps/radius/borders/blur) at build time, with the
  livery engine able to re-dispatch it live via hyprctl. Scope: LOOK +
  session plumbing only — host-invariant behavior (keybinds, input,
  tiling, window rules) lives in `modules/dendrites/hyprland.nix` instead.
- `quickshell/` — renders the complete shell surface (bar, notifications,
  launcher, osd, lockscreen, greeter, wallpaper, agentWidgets,
  sessionGraph). Component-tier fallback (null → palette) applies locally;
  never reads `song/` runtime paths at build time. The root `shell.qml`
  runs under `//@ pragma UseQApplication` (2026-08-23): platform dbusmenus
  (`QsMenuAnchor` — the bar tray's SNI menus) hard-error in the default
  QGuiApplication mode, and the pragma only takes effect on a service
  restart, not a reload.
- `stylix/` — the baked half of theming: one base16 scheme + fonts/cursor/
  wallpaper feed Stylix, which themes every nix-manageable target. The live
  half (`stage/livery.json` + hyprctl + OSC) is `song`'s job — both derive
  from the same `aoide.livery` so preview and adopted state can't diverge.

## What it consumes

Exactly `aoide.livery`, `aoide.arrangement`, `aoide.surfaces` from
`modules/nucleus/options.nix` — the closed whitelist. Nothing else, from no
other module.

## How it composes

Every facet self-gates on its own feature; a facet is a render surface
only — the paint test (CONTRACTS.md §0) decides whether a given file
belongs in a facet's `qml/` (song-blind, song-plural, bridge/mechanism) or
in a song's own `widgets/` instead.
