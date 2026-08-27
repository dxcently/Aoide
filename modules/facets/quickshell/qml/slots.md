# Per-song widget slots — catalog

Companion to CONTRACTS.md §5 ("Per-song flavor widgets"). That section owns
the mechanism (build walk, manifest, resolution, injected-prop contract);
this file owns the catalog — which slot names actually have a host anchor
wired today, so a song author knows what's real to dress.

## Two anchor kinds

Every slot below is hosted by one of two anchors:

- **`WidgetSlot`** — an `Item`. Sizes itself off the loaded child and parents
  it into an existing layout (a popout body, a repeater delegate, …). The
  loaded widget's root must be an `Item`.
- **`SurfaceSlot`** — a non-visual `QtObject`. For a slot whose root IS a
  `PanelWindow` — its own Overlay/Top layer surface, own WlrLayershell
  namespace, own keyboard focus, its own `GlobalShortcut`. `SurfaceSlot`
  does no visual parenting/sizing; it just resolves + creates the widget and
  exposes the live instance as `.item` for the host to call directly (e.g.
  the bar's clef calling `.toggle()`). A slot that roots a `PanelWindow`
  documents its namespace here — that contract (which layer, what
  identifies it to the compositor facet's layerrules) travels with the slot,
  same as extras/fallback.

Both anchor kinds resolve through the SAME baseline-fallback chain
(`StagingEngine.resolveSong`, CONTRACTS.md §5): the active song's own file if
it authored the slot, else **sonata's** (the shipped baseline every song
falls back to — mirrors `aoide.song`'s own default), else — `WidgetSlot`
only — the anchor's own facet-side `fallback` Component, else nothing.
`SurfaceSlot` slots have no facet-side fallback: sonata's file IS the floor,
since a window-owning slot moves as one whole unit with nothing left behind
in the facet to fall back to.

## The universal widget contract

Any `song/songbook/<name>/widgets/<slot>.qml` file that a song drops is
carried by the build (`modules/facets/quickshell/default.nix`) and becomes
resolvable by name — the build imposes no fixed slot enum. But **being
carried is not being rendered**: nothing shows on screen until a host
surface embeds a `WidgetSlot` anchor for that exact slot name (see the
table below). Dropping a file for a slot no anchor names yet is inert —
carried to disk, never loaded.

`aoide rice stage` now hot-syncs an edited **existing** song widget file
(and any helper/asset files alongside it) into `run/qml/songs/<song>/`
live, no rebuild — Quickshell's own file-watcher picks the edit up. A
**brand-new** slot file still needs `systemctl --user restart
aoide-quickshell.service` to be discovered (`manifest.json` is only read at
startup) — unchanged limitation.

Every widget QML file, whatever slot it fills, must follow this shape:

- **Root is an `Item`.** (`WidgetSlot` sizes itself off the loaded item's
  `implicitWidth`/`implicitHeight`, so a non-`Item` root breaks layout.)
- **Declares `required property var livery`** — the active song's
  `LiveryState`, injected by every anchor unconditionally.
- **Declares `required property var bridge`** — the `ShellBridge`, injected
  by every anchor unconditionally (`WidgetSlot` guards the fallback path,
  but a song widget is always given this).
- **Declares any slot-specific extras as `required property`** — e.g. the
  bar slot's `shared` (see table). Extras are per-slot, not
  universal; check the table for what a given slot's anchor passes.
- **Sizes itself via `implicitWidth`/`implicitHeight`** — the host
  positions the `WidgetSlot`, not the widget; the widget only needs to
  report its own footprint.
- **NEVER receives `config.*`.** A song widget is store-copied score,
  structurally incapable of reaching host/facet nix options through this
  surface — no anchor passes anything nix-shaped in, ever.

## Wired slots

| slot | anchor kind | host anchor | extras | fallback |
| --- | --- | --- | --- | --- |
| `calendar` | `WidgetSlot` | `bar.qml` (sonata) — the clock's calendar popout (`WidgetSlot { slot: "calendar" }`) | none | none — an unauthored `calendar` slot renders nothing (the popout itself gates on `stagingEngine.has(...)` before opening) |
| `notifications` | retired 2026-08-16 — the Quickshell `NotificationServer` surface (AoideNotifications.qml) is gone; dunst owns `org.freedesktop.Notifications`. Superseded by `herald` below, which draws the popup WITHOUT owning the bus name | — | — |
| `herald` | `SurfaceSlot` | `shell.qml` (facet) — `SurfaceSlot { slot: "herald" }`; the notification popup, drawn from `state/stage/herald.json` (dunst draws nothing — see `modules/dendrites/dunst.nix`) | none | none — sonata's `widgets/herald.qml` is the floor |
| `herald-center` | `WidgetSlot` | `AoidePanel.qml` (facet) — the dock's gadget column (`WidgetSlot { slot: "herald-center" }`), under the Conductor | none | none — sonata's own `widgets/herald-center.qml` IS the baseline floor; no facet-side Component fallback is wired |
| `powermenu` | `SurfaceSlot` | `shell.qml` (facet) — `SurfaceSlot { slot: "powermenu" }`; the bar's clef calls `.item.toggle()` | none | none — sonata's `widgets/powermenu.qml` is the floor |
| `launcher` | `SurfaceSlot` | `shell.qml` (facet) — `SurfaceSlot { slot: "launcher" }` | `clipboard` (the `AoideClipboard` instance), `ledger` (`GrimoireLedger` — stays in the facet, a data seam not chrome) | none — sonata's `widgets/launcher.qml` is the floor |
| `bar` | `WidgetSlot` | `shell.qml` (facet) — the bar's `PanelWindow` content, `WidgetSlot { slot: "bar" }` | `shared` (session-state QtObject), `powermenu` (the powermenu slot's live `.item`), `stagingEngine` (so the bar's own embedded calendar `WidgetSlot` can resolve) | none — sonata's `widgets/bar.qml` is the floor |

### Window-owning slot namespaces (`SurfaceSlot` contract)

| slot | WlrLayershell namespace | layer |
| --- | --- | --- |
| `powermenu` | `aoide-powermenu` | Overlay |
| `launcher` | `aoide-launcher` | Overlay |

The compositor facet's glass layerrules (`modules/facets/compositor/default.nix`)
match on these namespaces (`layerrule blur` + hyprglass) — the namespace
string is part of the slot's documented contract, not an implementation
detail a song widget is free to rename.

A slot is added to this table **only once a real anchor (`WidgetSlot` or
`SurfaceSlot`) is wired for it** — matching CONTRACTS.md §5's "what IS
built, not what's speculatively planned" discipline. A song authoring
`widgets/<slot>.qml` for a slot not yet in this table is carried by the
build (harmless, forward-compatible) but has no anchor to load it until one
is wired.

### Helper files (multi-file widgets)

A widget that needs its own helper components (e.g. `bar.qml` needs a
workspace-row component) drops them alongside it under the same song's
`widgets/` dir, with an **uppercase** filename — QML's own type-file
convention marks these as helper components, not slots. The build
(`modules/facets/quickshell/default.nix`) copies the WHOLE `widgets/` dir
per song (helper files, asset subdirs, all of it) but only manifests
top-level **lowercase-kebab** `.qml` files as slots; `.gitkeep` is always
skipped. A helper file is carried to disk but never independently
resolvable as a slot.

### Facet or song?

**The facet is not a component library.** The destination for a shared
visual component is the song's `widgets/` dir with an uppercase name —
the helper mechanism directly above — not this directory.

`CONTRACTS.md §0`'s test runs first and decides **bridge vs QML**: delete
every `.qml` in the repo; if the capability is no longer reachable from a
terminal, it was never QML's to hold. The test below decides **facet QML
vs song QML**, and only applies to a file that already passed §0.

> ### The paint test
>
> A file stays in `modules/facets/quickshell/qml/` **iff all three are YES**:
>
> 1. **Song-blind.** Does the file name zero aesthetic decisions? Reading
>    `livery.paletteFg` is fine — that is picking up an API. *Deciding* that a
>    gauge is drawn `[▓▓░░]`, that a frame wears a pediment, that a face is a
>    kaomoji, that a morph takes 340ms because that is the house tier, or that
>    the accent cycle runs rust→murex→aegean — those are not.
> 2. **Song-plural.** Would a second, unrelated song use this file **unchanged**?
>    Not "could be adapted to". Unchanged.
> 3. **Bridge or mechanism.** Is its job one of exactly three: (a) publish system
>    or aoide state as **data**; (b) resolve, host, or inject a song's own QML;
>    (c) be the process entry point that wires (a) into (b)? "It draws something
>    reusable" is not a fourth category.
>
> Any NO → it belongs in `song/songbook/<song>/widgets/`.
>
> **Tie-breaker**, when an agent honestly cannot call question 2: *would a
> reviewer file this file's diff under "design change"?* If yes, it is song. A
> facet file's diff is always a mechanism change.
>
> **What the test is not.** It is not "does it paint" — `WidgetSlot` is an
> `Item` and `SurfaceSlot` hosts a `PanelWindow`, and both are facet. It is not
> "is it a `QtObject`" — `MoodFaces` and `MorphState` are `QtObject`s and both
> are song. It is not line count — `AudioColonnade` is 1940 lines of song and
> `AoideIpc` is 24 lines of facet. It is not "is it shared" — shared across
> *widgets* is not shared across *songs*, and only the second earns a facet home.
>
> **The corollary for a new API**: a new capability lands as a facet bridge that
> answers **with data**, never with a component to instantiate, and the song
> picks it up by name. Concretely: a facet bridge exposes `paletteAccent`; it
> does not expose `ctxBar()`. If the natural shape of the new thing is "a
> component every widget instantiates", it is not an API — it is a song helper,
> and it goes in `widgets/` with an uppercase name.

Question 3 is the load-bearing one, and it is where every genuine argument in
this tree lives.

## Declared slots — the widget-type registry

Everything above is the **anchored** catalog: a slot name a host surface
already wired a `WidgetSlot`/`SurfaceSlot` for. This section is a second,
independent mechanism — a **declared** slot: a song registers a brand-new
slot name via nix, apart from the fixed catalog above, instead of merely
dressing a name the facet already anchored.

`aoide.arrangement.widgets.<slot>` (`modules/nucleus/options.nix`, sibling
of `aoide.livery`) is the registration. It is twin-written into that song's
`livery.json` under a flat top-level `.widgets` key — the same twin-write
pattern every other song field uses — and validated strictly by `rice lint`
(closed, kind-conditional field rejection;
`pkgs/aoide/crates/song/src/livery/schema.rs`). The slot's QML body still
lives at the normal `song/songbook/<name>/widgets/<slot>.qml` path, same
convention as every slot in the table above — registering a slot's TYPE and
authoring its body are two separate acts, same as the anchored catalog.

Each entry names a `kind`; the two kinds' extra fields are mutually
exclusive (`rice lint` rejects a field from the wrong kind):

| kind | fields | renders |
| --- | --- | --- |
| `surface` | `namespace` (nullOr str; derives `aoide-<slot>` when null), `layer` (`overlay` \| `top`, default `overlay`), `shortcut` (nullOr str — a `GlobalShortcut` name, split on `:` into appid/name), `blur` (bool, default true) | its own `PanelWindow`/layer-shell surface — hosted by `SongSurfaces.qml`, a non-visual `Instantiator` of `SurfaceSlot`s, one per declared surface-kind entry the active song carries |
| `dock` | `order` (nullOr int, sort key; default treated as 0) | an `Item` mounted into `AoidePanel`'s gadget column — hosted by `SongGadgets.qml`, a `Repeater` of `WidgetSlot`s, sorted `(order ?? 0, slot-name)` ascending; mounted as the column's last children, after the shipped gadgets and `herald-center` |

`order` exists because the registry is serialized through
`serde_json::Value`/`BTreeMap` (no `preserve_order` feature), so a song's
declared widget keys do NOT preserve authored order between the build-time
nix walk and the native hot-sync — `order` is the only way to get
deterministic layout among multiple `dock` entries. It only sorts declared
`dock` entries against each other; it does not interleave a `dock` entry
among the shipped gadgets ahead of it in the column.

**Injected-prop contract — unchanged, still just `livery` + `bridge`.** Both
`SongSurfaces.qml` and `SongGadgets.qml` pass the same fixed pair every
anchor above does — deliberately WITHOUT `shared` (the session-state
QtObject only the `bar` slot receives): a declared widget is store-copied
score like any other slot body, no wider surface than the rest of this
contract grants.

**Bodyless-slot warning.** A song can declare a slot's TYPE with no actual
`widgets/<slot>.qml` body — neither the active song nor the sonata baseline
carries one. Both hosts detect this via the same `resolvedSong === ""`
signal the anchored catalog's own resolution already computes:
`SongSurfaces.qml` reuses `SurfaceSlot`'s own existing
`console.warn("[aoide/surfaceslot] no song (active or baseline) provides
slot", …)` path unchanged; `SongGadgets.qml` warns at
`console.warn("[aoide/songgadgets] no song (active or baseline) provides
dock slot", …)`. Either way: a console warning, nothing rendered — a
declaration with no body is inert, not an error.

`song/songbook/etude/` is the worked example: `rice.nix` declares
`aoide.arrangement.widgets.demo = { kind = "surface"; namespace =
"aoide-etude-demo"; layer = "overlay"; shortcut = "aoide:etude-demo"; blur =
true; }`, `widgets/demo.qml` is its `PanelWindow`-rooted body — proving
build-walk → `rice lint` → `rice stage`/`preview` hot-sync →
`SongSurfaces.qml` end to end. No committed song declares a `kind: "dock"`
entry yet; `SongGadgets.qml`'s `Repeater` holds zero delegates in practice
today, a structural no-op rather than an accident of which song is active.
