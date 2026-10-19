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
   (or `pkgs/aoide/crates/strillix-rice` scaffolds with `aoide rice compose`).
2. Put the palette, polarity, window and geometry in `system.module`, under
   `aoide.theme`, all inside one `config = lib.mkDefault { … };`. Nothing else
   in the `module` but `aoide.theme` and `aoide.layout`.
3. Keep the ten providers in `system.providers` as `"strillix"` (or a provider a
   consumer adds). Keep `home.members = [ "strillix" ]`.
4. Write `hyprland.conf` beside it: the binds, unbinds, layout and animations.
   An empty file (a header only) is valid. The gaps, rounding, borders, blur
   and colours are the theme's: `checks.rice-shape` refuses them in this file.
5. Name the rice's module arguments by what they are: `lib` (for `lib.mkDefault`),
   `rice` and `borrow`, and `pkgs` only when used. `{ lib, ... }:` is the usual
   shape for a module that sets only theme.
6. Remove any self-gate: a body is read only when its rice is performed, so
   `config.aoide.rice == "<name>"` has no place in it.
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
`pkgs/strillix-rices`, `aoideSeedRices` and the offline generator assume — and
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
aoide rice preview <name>
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

- **Option namespace:** the option namespace moved to `aoide.theme.*`. A
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

Old rice state moves into this layout on a rebuild (the activation's
`aoideMigrateRoots`) or by `strx migrate`. Until it settles, `strillix`
refuses every verb except `migrate`, help and version. The per-file rules
and refusals are in CONTRACTS.md §4; the change record is in CHANGELOG.md.

## What this playbook does NOT cover

The other four surfaces named in early per-rice-widget sketches — `greeter`,
`lockscreen`, `osd`, `nowPlaying` — have **no host anchor built** and are not
part of the live slot enum. See `rices/sonata/design/greek-grammar.md`
§7 for that as-built/not-built boundary.
