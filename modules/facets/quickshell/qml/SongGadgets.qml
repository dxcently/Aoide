// SongGadgets.qml — visual host for the active song's DECLARED dock-kind
// widgets (CONTRACTS.md §5's "Per-song flavor widgets"; `aoide.arrangement
// .widgets`, `modules/nucleus/options.nix` — the `kind = "dock"` v2
// expansion of the same declared widget-type registry `SongSurfaces.qml`
// hosts the `kind = "surface"` half of). Same data source
// (`stagingEngine.declaredWidgets(song)`), same overall shape as
// SongSurfaces.qml, but the opposite mount: a `dock` entry has no layer
// surface of its own (the compositor facet already filters it out of
// layerrule generation) — it mounts as a plain Item straight into
// AoidePanel's existing gadget column, alongside the shipped
// ConductorGadget/TerminalsGadget/etc.
//
// Rooted at Repeater, not Instantiator: SongSurfaces.qml hosts non-visual
// `SurfaceSlot` QtObjects, so Instantiator (QtQml.Models) is the only tool
// that fits. This is the opposite case — a VISUAL delegate list going
// straight into `AoidePanel`'s `Column { id: stack }` — so `Repeater` is
// correct: it's `transparentForPositioner`, so zero delegates (today, since
// no committed song declares a `kind: "dock"` entry) contributes zero
// footprint and zero spacing, matching every other `Repeater`-in-a-`Column`
// idiom already in this tree (e.g. sonata's `bar.qml`).
//
// `order` (options.nix `widgetType.order`, dock-only): the registry is
// serialized through `serde_json::Value`/`BTreeMap` on the Rust side (no
// `preserve_order`), so a song's declared widget keys do NOT preserve
// authored order between the build-time nix walk and the native hot-sync —
// `order` is the only deterministic layout signal among multiple `dock`
// entries. Sorted here explicitly by `(order ?? 0, slot-name)`, both
// ascending, order first with slot name as the tie-break — never relying on
// `Object.keys()` iteration order.
//
// Delegate: the real `WidgetSlot.qml` component directly (the same one
// `herald-center` uses in AoidePanel.qml) — resolve/load logic is not
// reimplemented here. Fixed injected-prop contract (CONTRACTS.md §5
// containment): `livery` + `bridge` only, nothing else — no `extraProps`,
// and that omission is load-bearing, not a style choice. `WidgetSlot
// ._rebuild()` has no idempotence guard the way `SurfaceSlot` had to grow
// one after an `extraProps` object-literal binding caused a destroy/rebuild
// storm (see `SurfaceSlot.qml`'s `_builtSource`/`_sameExtras` guards) — an
// `extraProps` binding here would risk the same storm for no reason, since
// dock gadgets get the same fixed prop contract as everything else declared
// so far, not the wider `livery+bridge+shared` the shipped facet gadgets get.
//
// A declared dock slot whose declaring song has no actual
// `widgets/<slot>.qml` body (neither the active song nor the sonata
// baseline) resolves through WidgetSlot's own existing fallback chain and
// renders nothing — WidgetSlot itself stays silent in that case (it only
// warns on a load/create failure, never on "no song provides this slot").
// So both declared kinds fail loudly the same way, this warns explicitly at
// the `[aoide/songgadgets]` prefix, mirroring `SurfaceSlot.qml`'s
// `[aoide/surfaceslot]` "no song (active or baseline) provides slot" warning,
// keyed off the same signal WidgetSlot itself computes: `resolvedSong === ""`.

import QtQuick

Repeater {
    id: root

    required property var livery
    required property var bridge
    required property var stagingEngine
    property real gadgetW: 360

    // [{ slot, decl, order }, …] — every dock-kind entry the active song
    // declares, sorted by (order ?? 0, slot-name) ascending. Empty for a
    // song with no declared dock widgets (every committed song today), so
    // the Repeater below holds zero delegates: a structural no-op, not an
    // accident of which song happens to be active.
    readonly property var dockEntries: {
        var decl = root.stagingEngine.declaredWidgets(root.livery.songName)
        var keys = Object.keys(decl)
        var out = []
        for (var i = 0; i < keys.length; i++) {
            var slot = keys[i]
            var w = decl[slot]
            if (w && w.kind === "dock") {
                var order = (w.order === undefined || w.order === null) ? 0 : w.order
                out.push({ slot: slot, decl: w, order: order })
            }
        }
        out.sort(function (a, b) {
            if (a.order !== b.order) return a.order - b.order
            return a.slot < b.slot ? -1 : (a.slot > b.slot ? 1 : 0)
        })
        return out
    }

    model: root.dockEntries

    delegate: WidgetSlot {
        id: dockSlot
        required property var modelData

        width: root.gadgetW
        slot: modelData.slot
        livery: root.livery
        bridge: root.bridge
        stagingEngine: root.stagingEngine

        onResolvedSongChanged: dockSlot._warnIfUnresolved()
        Component.onCompleted: dockSlot._warnIfUnresolved()

        function _warnIfUnresolved() {
            if (dockSlot.resolvedSong === "") {
                console.warn("[aoide/songgadgets] no song (active or baseline) provides dock slot", dockSlot.slot)
            }
        }
    }
}
