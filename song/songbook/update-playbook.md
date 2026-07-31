# Songbook update playbook

Referenced by `CONTRACTS.md` §5. Practical steps for authoring/updating a
song — notes today, plus the two live slots the staging engine resolves.

## Notes (`drachma.json` / `rice.nix`)

A song's `rice.nix` self-gates on `config.aoide.song == "<name>"` and sets
**only** `aoide.drachma` (palette + component tiers, optionally `wallpaper`).
See an existing song (`song/songbook/default/rice.nix`,
`song/songbook/sonata/rice.nix`) for the shape. `drachma.json` alongside it is
the same values as a hand/agent-maintained JSON file — kept in sync with
`rice.nix`, not derived from it.

## Per-song flavor widgets (`widgets/<slot>.qml`)

The **two currently-live slots** are `calendar` and `notifications`
(CONTRACTS.md §5). To dress one:

1. Create `song/songbook/<name>/widgets/<slot>.qml`.
2. Declare the fixed injected-prop contract: `required property var notes`
   and `required property var bridge` (even if `bridge` goes unused — every
   song widget carries both, so the resolver's creation-time property
   assignment always has somewhere to put them). A slot may also receive
   extras — `notifications` additionally supplies `notification`.
3. Size via `implicitWidth`/`implicitHeight` — the host anchor
   (`WidgetSlot.qml`) sizes itself to whatever loads.
4. Colour **only** from `notes.*` roles (`paletteBg/Fg/Accent/Urgent/Hot`,
   the `base16`-derived accents) — never a hardcoded hex, so the widget stays
   correct if the song's palette changes later.
5. Nothing else is reachable: a widget sees `notes` + `bridge` (+ declared
   extras) only, never nix `config.*` (CONTRACTS.md §5 containment).

**Build carry-over:** on the next `nixos-rebuild`, the quickshell facet's
derivation copies the new file to `$out/qml/songs/<name>/<slot>.qml` and adds
`<name>` → `[…, "<slot>"]` to the generated `songs/manifest.json` — every
committed song's slot files are carried at once, not just the active one.

**Live preview, no rebuild:** once a slot file exists anywhere in
`$out/qml/songs/` (i.e. after at least one rebuild has carried it), switch
which song is *active* live with:

```
aoide rice preview <name>
```

This stages `<name>`'s `drachma.json` (with `song` injected) into
`song/stage/drachma.json`; `DrachmaState.qml` hot-reloads it, and every
`WidgetSlot` re-resolves against the new `songName` — a song's calendar/
notifications body swaps with no restart, exactly like its colours do.

An unauthored slot has no fallback for `calendar` (the bar's popout simply
doesn't open) and falls back to the shared chrome for `notifications` (the
existing `NotificationCard`).

## What this playbook does NOT cover

The other four surfaces named in early per-song-widget sketches — `greeter`,
`lockscreen`, `osd`, `nowPlaying` — have **no host anchor built** and are not
part of the live slot enum. See `song/songbook/sonata/design/greek-grammar.md`
§7 for that as-built/not-built boundary.
