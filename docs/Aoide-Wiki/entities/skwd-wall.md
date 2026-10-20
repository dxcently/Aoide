---
type: entity
created: 2026-09-28
updated: 2026-10-12
tags: [aoide, wallpaper, paint, engine, provider]
source: "[[entities/theme]]"
---

# skwd-wall

An external wallpaper engine — picker, control daemon and renderer — that
paints a user's PICKS: images, videos and Wallpaper Engine scenes, each on its
own layer-shell surface. It never paints a rice's own DEFAULT wallpaper; it is what
a host can choose to paint what the user picked.

Aoide takes it from the flake input (`flake.nix`, branch `v2` of
`github:liixini/skwd-wall`) and ships three of its packages — the picker
(`skwd-wall-v2`), the control daemon (`skwd-deck`, which carries the `skwd-helm`
client) and the renderer (`skwd-paper`) — never the suite `default`, which drags
the semantic-search lens and its model pack.

## How Aoide uses it

It is one alternative of the `wallpaper` capability
(`modules/dendrites/wallpaper/`), the way Hyprland is one alternative of the
`compositor`. A host picks it with
`habit.dendrites.wallpaper.provider = "skwd-wall"`; the provider file that
answers installs the packages, runs one `skwd-walld` user service of its own
(never the upstream NixOS module, which installs the whole suite and defines a
second unit of the same name), and enforces the engine config keys Aoide owns
at activation — the engine rewrites its `config.json` in place, so that file is
seeded under `aoide.root` and is never a store symlink.

**Aoide owns the colours; the engine's theming is off** (`theme.policy = "off"`,
the whole-fan-out switch). Its library never indexes the user's Pictures folder
(`paths.wallpaper` and `paths.videoWallpaper` are pinned inside the lane's own
state directory), `paper.engine = "skwd-paper"` is the bundled renderer rather
than the `awww` alternative, and `paper.wallpaperLayer = "background"` puts it
on the same wlr-layer-shell layer as Aoide's own wallpaper surface — the layer
key is the one thing that decides which of the two is in front of the other.

## The two directions

- **Aoide → engine.** `strx wallpaper sync` (CONTRACTS.md §4) is the only write:
  the staged pick is applied (`skwd-helm apply <path|we:<id>> -o '*'`) while one
  applies to the staged rice, and steps aside otherwise: the provider's own
  `clear` verb where the installed release has one, and a 1x1 fully transparent
  PNG applied to every output otherwise (the step-aside image, since the release
  Aoide first shipped against has no `clear`). It is the one door that waits for
  a provider which has not answered yet — up to ~5 s, because the unit's
  `ExecStartPost` races the daemon's own socket — while `wallpaper set`, `--clear`,
  `rice stage` and `rice back` each make a single attempt. It is what the
  lane's unit runs in `ExecStartPost`, and what every pick write runs. Only
  the identity crosses: a still's `tile` and `position` (CONTRACTS.md §4) are
  the shell's own layer's to paint, so the engine lays out a picture named
  `*_tile*` or set with `--position tile` the way it lays out every other.
- **Engine → Aoide.** The daemon's `postProcessing` hook runs exactly one
  command, `strx wallpaper set --from-skwd --kind %type% %path%`, which RECORDS what
  the engine now shows and applies nothing back — that asymmetry is what makes
  the round trip terminate (the engine is source-blind: it cannot tell an Aoide
  apply from the user's own pick, so a hook that re-applied would loop
  unbounded), and it records no `tile` or `position`. This door is also the
  ONE writer exempt from the `declarative` lock: a pick made in the engine's
  own picker is recorded while nix owns the declared state, because what shows
  IS the pick (CONTRACTS.md §4).

The shell's own layer stands down while the engine has a pick on screen: the
`wallpaper` surface stays mapped and transparent, and `AoideWallpaper.qml`
paints nothing at all for it. That is the one reason `wallpaper.owner` stays
`quickshell` in the `strillix.surfaces` registry — the engine's paper surface is
the compositor's business, not an `aoide-<slot>`.

## What is not verified

Anything only a live session can show: the engine's surface ordering against
Quickshell's layer, the hook firing under the real daemon, and the cost of a
video or a scene while rendering.
