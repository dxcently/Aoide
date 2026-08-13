# Per-song widget slots — catalog

Companion to CONTRACTS.md §5 ("Per-song flavor widgets"). That section owns
the mechanism (build walk, manifest, resolution, injected-prop contract);
this file owns the catalog — which slot names actually have a host anchor
wired today, so a song author knows what's real to dress.

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

| slot | host anchor | extras | fallback |
| --- | --- | --- | --- |
| `calendar` | `AoideBar.qml` — the clock's calendar popout (`WidgetSlot { slot: "calendar" }`) | none | none — an unauthored `calendar` slot renders nothing (the popout itself gates on `stagingEngine.has(...)` before opening) |
| `notifications` | `AoideNotifications.qml` — each card in the notification stack's `Repeater` (`WidgetSlot { slot: "notifications" }`) | `notification` (the tracked `Notification` model item) | the shared `NotificationCard.qml`, via the anchor's `fallback: notifCardComp` |

A slot is added to this table **only once a real `WidgetSlot` anchor is
wired for it** — matching CONTRACTS.md §5's "what IS built, not what's
speculatively planned" discipline. A song authoring `widgets/<slot>.qml`
for a slot not yet in this table is carried by the build (harmless,
forward-compatible) but has no anchor to load it until one is wired.
