# Rices update playbook

Referenced by `CONTRACTS.md` §5. Practical steps for authoring/updating a
rice — its theme, plus the slots the staging engine resolves for it.

## Rice bodies (v0 to v1: `rice.nix` as a body)

A rice's `rice.nix` is an attrset, the body the constructor reads as data:
`system` (the paint: the ten `strillix` providers and the `module` of theme and
layout) and `home` (the `strillix` renderer and `rice.hyprland.conf`). This is
the migration of a v0 rice (`CONTRACTS.md` §5, "Enforcement"), and the step list
for a new rice is the same:

1. Write `rice.nix` as the body in the shape `rices/sonata/rice.nix` has
   (or `pkgs/aoide/crates/strillix-rice` scaffolds with `strx rice compose`).
2. Put the palette, polarity, window and geometry in `system.module`, under
   `strillix.theme`, all inside one `config = lib.mkDefault { … };`. Nothing else
   in the `module` but `strillix.theme` and `strillix.layout`.
3. Keep the ten providers in `system.providers` as `"strillix"` (or a provider a
   consumer adds). Keep `home.members = [ "strillix" ]`.
4. Write `hyprland.conf` beside it: the binds, unbinds, layout and animations.
   An empty file (a header only) is valid. The gaps, rounding, borders, blur
   and colours are the theme's: `checks.rice-shape` refuses them in this file.
5. Name the rice's module arguments by what they are: `lib` (for `lib.mkDefault`),
   `rice` and `borrow`, and `pkgs` only when used. `{ lib, ... }:` is the usual
   shape for a module that sets only theme.
6. Remove any self-gate: a body is read only when its rice is performed, so
   `config.strillix.rice == "<name>"` has no place in it.
7. Run `checks.rice-shape` (`nix build .#checks.x86_64-linux.rice-shape`) and
   `tests/selection/run.sh`.

The `theme.json` beside the body is the same values as a hand- or agent-
maintained JSON file, kept in sync with `system.module`, not derived from it.

A rice's `rice.nix` is handed two things by ARGUMENT and never by a path
(`CONTRACTS.md` §5): `rice` — the `lib/rice.nix` API — and `borrow`, where
`borrow "sonata"` is sonata's rolled-up `_widgets/` (the one cross-rice idiom).
A shelf's `_widgets/default.nix` is `{ lib, rice, borrow, ... }:`, and takes
only the arguments it uses. A `../` path literal in any `.nix` under a rice
folder breaks every machine whose rices is not this checkout — which is what
`pkgs/strillix-rices`, `strillixSeedRices` and the offline generator assume — and
`checks.rice-shape` scans for it.

## Per-rice flavor widgets (`widgets/<slot>.qml`)

**Which slots are live is not listed here.**
`pkgs/strillix-shell/qml/slots.md` is the catalog and the only place
that tracks it — which names have a host anchor wired, which anchor kind
each uses, and what extras each one is passed. A slot list copied into this
file goes stale the moment an anchor is added or retired, which is exactly
what happened to the pair this section used to name. Read the catalog, then
come back here for the steps. To dress a slot:

1. Create `rices/<name>/widgets/<slot>.qml`.
2. Declare the fixed injected-prop contract: `required property var theme`
   and `required property var bridge` (even if `bridge` goes unused — every
   rice widget carries both, so the resolver's creation-time property
   assignment always has somewhere to put them). A slot may also receive
   extras; the catalog's table says which, per slot.
3. Size via `implicitWidth`/`implicitHeight` — the host anchor
   (`WidgetSlot.qml`) sizes itself to whatever loads. A widget that fills its
   host instead of reporting a footprint binds `width`/`height` to `parent`
   directly; `sonata/widgets/bar.qml` documents why the anchor cannot do this
   for you.
4. Colour **only** from `theme.*` roles (`paletteBg/Fg/Accent/Urgent/Hot`,
   the `base16`-derived accents) — never a hardcoded hex, so the widget stays
   correct if the rice's palette changes later.
5. Nothing else is reachable: a widget sees `theme` + `bridge` (+ declared
   extras) only, never nix `config.*` (CONTRACTS.md §5 containment).

**Every bar embeds `ricemode`.** The rice-mode control is one slot, and every
`bar.qml` carries `WidgetSlot { slot: "ricemode" … }`, spelled exactly
`slot: "ricemode"` (the check reads that literal). The slot resolves through
the baseline chain below: a rice with no `widgets/ricemode.qml` is dressed by
sonata's, and a rice may author its own to put the control in its own grammar,
provided it keeps the contract: any click opens the rice menu over
`bridge.riceMenu(cb)` (the cell itself never switches), a rice row calls
`bridge.riceMode("stage", name)`, the declarative row calls
`bridge.riceMode("declarative")`, the draft rows call
`bridge.riceDraft(action, name)`, and a bridge without `riceMenu` shows no
menu. `checks.bar-ricemode` fails a bar body that omits the embed, and a
rices whose sonata has no `ricemode`. It reads the committed rices, so a
rice that exists only in the runtime rices is checked when `rice declare`
brings it into the checkout.

**Build carry-over:** on the next `nixos-rebuild`, the strillix lane's
derivation copies the new file to `$out/qml/rices/<name>/<slot>.qml` and adds
`<name>` → `[…, "<slot>"]` to the generated `rices/manifest.json` — every
committed rice's slot files are carried at once, not just the active one.

**Live preview, no rebuild:** once a slot file exists anywhere in
`$out/qml/rices/` (i.e. after at least one rebuild has carried it), switch
which rice is *active* live with:

```
strx rice stage <name>
```

This stages `<name>`'s `theme.json` (with `rice` injected) into
`/stage/theme.json`; `ThemeState.qml` hot-reloads it, and every
`WidgetSlot` re-resolves against the new `riceName` — a rice's slot bodies
swap with no restart, exactly like its colours do.

What an unauthored slot falls back to is per-slot, and the catalog's
`fallback` column is where that lives. The general chain is: the active
rice's own file, else sonata's (the shipped baseline), else — `WidgetSlot`
only — the anchor's lane-side `fallback` Component, else nothing rendered.

## Migration — the theme rename

The theme merge (THEME-MERGE.md) renamed the note engine and its whole
surface. The note schema is unchanged (v0), so this was a namespace/file
rename, not a content migration:

- **Option namespace:** the option namespace moved to `strillix.theme.*`. A
  `lib.mkRenamedOptionModule` alias in `modules/nucleus/options.nix` kept any
  out-of-tree host or stale rices `rice.nix` that still set the old name
  evaluating during the transition; the alias was dropped when Phase 4 closed
  the window.
- **Rices data file:** the per-rice theme file is now
  `rices/<rice>/theme.json`.
- **Live stage file:** `/stage/theme.json` is canonical. During the
  transition window writers mirrored to a legacy stage name and readers fell
  back to it when `theme.json` was absent, so a running desktop never read a
  missing stage file; the mirror + fallback were dropped when Phase 4 closed
  the transition window.

## The strillix layout (stage v1, history v1, drafts v1)

The rice runtime lives under `$STRILLIX_ROOT` (default `~/.strillix`), not
under aoide's root. Its format versions are `stage/mode.json`,
`stage/theme.json`, `stage/wallpaper.json`, `state/stage/notifications.json`
and each `rices/<rice>/history/NNNN.json`, all `schemaVersion` `"1"`. The
rice folder layout in `rices/<rice>/drafts/<name>/` is v1 (CONTRACTS.md §4).
The note schema (§1) and the rice body (§5) are unchanged.

- `$STRILLIX_ROOT/stage` is the live stage; `declared/`, `rices/`,
  `wallpapers/` and `rices-aside/` sit beside it.
- A rice keeps `theme.json`, `wallpaper/` and `history/` in its folder, and
  each draft keeps the same names.
- `mode.json`'s key for the active rice is `rice`.

Strillix's trees found under aoide's root (`run/qml`, `run/elements`,
`state/skwd-wall`, `state/captures`, `state/pointer-pos.json`) move into
this layout on every switch (the activation's `strillixMigrateRoots`) or by
`strx migrate`; the per-entry rules are in CONTRACTS.md §4. Nothing moves
rice state from the older places (a `song` directory under `$AOIDE_ROOT`
or `~/Aoide`, and `~/.local/state/lyra`); it stays where it is. The
change record is in CHANGELOG.md.

## Rice shape v1 to v2: the names a rice shares say strillix

v2 renames the names a rice shares with the shell and the compositor. No
shape changes, and nothing answers to the old names:

| v1 | v2 |
|---|---|
| layer namespace `aoide-<slot>` (`aoide-bar`, `aoide-dock-scrim`, …) | `strillix-<slot>` |
| `appid: "aoide"`, a declared `shortcut = "aoide:<name>"`, a bind `global, aoide:<name>` | `appid: "strillix"`, `"strillix:<name>"`, `global, strillix:<name>` |
| `AOIDE_WALLPAPER` | `STRILLIX_WALLPAPER` |
| `AOIDE_RIG_PEEK`, `AOIDE_RIG_BATTERY` | `STRILLIX_RIG_PEEK`, `STRILLIX_RIG_BATTERY` |
| the shell's `AoideClipboard`, `AoideWallpaper`, `AoideWallpaperPicker` | `StrillixClipboard`, `StrillixWallpaper`, `StrillixWallpaperPicker` |

To migrate a rice:

1. Rename every v1 name in the files it runs, comments included: each `.qml`,
   `.js`, `.nix`, `.json` and `.conf` outside any `design/` folder, its
   `rice.nix`, `theme.json`, `hyprland.conf` and `_widgets/` among them.
2. Run `checks.rice-shape`: it names each of those files that still spells a
   namespace, a shortcut or a variable the v1 way. An old type name is not
   scanned; QML refuses it when the widget loads.

A machine copy under `$STRILLIX_ROOT/rices` is renamed the same way, but
`checks.rice-shape` reads only the checkout's `rices/`: it sees a machine copy
once `strx rice declare` has copied it there. At the switch, `strx rice
refresh` brings a built-in rice's files up to the shipped ones where the
machine has not changed them since refresh last wrote them. The rest is
renamed by hand: a file the machine edited, a file refresh has no record of,
and every file of a rice that is not built in. `strx rice mode stage <rice>`
then stages the renamed copy. Until then no bind calls the rice's own
shortcuts, its surfaces lose their glass, a widget that reads the baked
wallpaper gets none, a rig variable set by its v2 name does not reach it, and
the healthcheck may call the desktop `blank`.

A venue that binds the shortcuts or lists the namespaces itself (keybinds,
layerrules, hyprglass) renames them in the same update that brings v2 in.

## What this playbook does NOT cover

The other four surfaces named in early per-rice-widget sketches — `greeter`,
`lockscreen`, `osd`, `nowPlaying` — have **no host anchor built** and are not
part of the live slot enum. See `rices/sonata/design/greek-grammar.md`
§7 for that as-built/not-built boundary.
