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

Every widget QML file, whatever slot it fills, must follow this shape:

- **Root is an `Item`.** (`WidgetSlot` sizes itself off the loaded item's
  `implicitWidth`/`implicitHeight`, so a non-`Item` root breaks layout.)
- **Declares `required property var notes`** — the active song's
  `LiveryState`, injected by every anchor unconditionally.
- **Declares `required property var bridge`** — the `ShellBridge`, injected
  by every anchor unconditionally (`WidgetSlot` guards the fallback path,
  but a song widget is always given this).
- **Declares any slot-specific extras as `required property`** — e.g.
  notifications' `notification` (see table). Extras are per-slot, not
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
| `notifications` | `WidgetSlot` | `AoideNotifications.qml` (facet) — each card in the notification stack's `Repeater` (`WidgetSlot { slot: "notifications" }`) | `notification` (the tracked `Notification` model item) | none — sonata's own `widgets/notifications.qml` IS the baseline floor; no facet-side Component fallback is wired |
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
