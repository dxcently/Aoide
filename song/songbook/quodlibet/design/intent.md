# quodlibet — Design Intent

**Rice:** quodlibet
**Thesis:** quodlibet unifies the palette and deliberately does not unify the
grammar — one ground, two hands, which is what a quodlibet is.

---

## What this song is

A quodlibet is the musical form defined by borrowing: a piece assembled from
other people's existing tunes sounded together (Bach, Goldberg var. 30). This
song owns no widget bodies at all. Its composition is plain attrset update
over its two parents' already-resolved widget maps:

```nix
sonataWidgets // { inherit (fugueWidgets) bar herald; }
```

14 slots. `bar` and `herald` are fugue's (the two slots fugue authors, kept
whole rather than split). The other twelve — `calendar`, `conductor`, `dock`,
`herald-center`, `launcher`, `meters`, `power`, `powermenu`, `terminals`,
`usage`, `wallpaper`, `wallpaper-picker` — are sonata's. quodlibet contributes
no new QML anywhere; only this file, `rice.nix`, and `livery.json`.

## The honest consequence

sonata's grammar is marble and ornament — pediments, rounded glass, cast
shadow. fugue's grammar is a flat lattice — hard rectangles, hairlines, no
radius or shadow. On one violet ground they will not look like one design.
That is expected, not a defect to be discovered in review: the lattice bar
is fugue's, the marble dock is sonata's, and the seam between them is the
point. A quodlibet does not smooth its voices into one line; it lets them
sound together, distinct.

## The palette

Both parents sit at extreme polarity: sonata's ground is `#f2ebde` (pale
marble, L* ~93), fugue's is `#0d0f12` (near-black graphite, L* ~4). Merely
choosing "a different colour" would catch a leak from one parent and miss
the other — a dark quodlibet makes a fugue leak plausible at a glance, a
light one makes a sonata leak plausible.

quodlibet's ground instead sits at MID luminance: `#5f4694`, a mid imperial
violet, roughly L* 36 — more than 30 L* away from both parents' grounds.
Mid luminance is the one property neither parent can imitate, it survives a
greyscale render, and it is checkable with arithmetic rather than taste: a
near-black or near-white widget on this ground is wrong from across the
room, no colour judgement required.

Every hex in this song's `livery.json` is distinct from every hex in
sonata's and fugue's — not required by any mechanism, but it turns "did the
right palette reach this widget" into a pixel sample instead of an opinion:
sample any borrowed widget's ground; `#f2ebde` means sonata leaked,
`#0d0f12` means fugue leaked, `#5f4694` means correct.

`base02` (`#452e6b`) is pinned one step below the ground specifically
because fugue's `Cell.qml` paints its hairline from
`livery.windowBorderInactive`, and fugue's own `rice.nix` pins that to
`base02` so the separator recedes rather than standing out. quodlibet keeps
the same relationship so a borrowed fugue cell's hairline still recedes on
the new ground.

`base03`/`base04` step UP from the ground rather than down — on a mid
ground a "muted" tone has to lighten to stay legible, which neither parent's
ramp (each stepping away from an extreme) needed to do.

## Slots dressed

None, in the QML sense — quodlibet ships no `widgets/` directory. All 14
slots resolve through the owner map to a foreign song: 12 to sonata, 2 to
fugue. `packages` is empty (`[]`): fugue's borrowed `bar` shells out to
nothing, and sonata's own `bar` record is entirely superseded, packages and
all, by the `//` override rather than merged into.

## Cover

`aoide.livery.wallpaper = null` — the stylix facet bakes a deterministic
solid from `palette.bg`, same mechanism sonata and fugue use.
