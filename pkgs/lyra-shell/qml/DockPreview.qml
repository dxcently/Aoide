import QtQuick
import Quickshell

// Standalone harness for the dock's own body — the ExodosPreview/
// CalendarPreview precedent, adapted to a THIRD shape. Like powermenu.qml
// (ExodosPreview's target), the dock body roots a `PanelWindow` — its own
// surface, not an `Item` a `Loader` can parent into — so this uses the same
// `Qt.createComponent` + `Component.createObject(null, props)` mechanism,
// no `Loader`. Like calendar.qml (CalendarPreview's target), the dock body
// is per-song score (song/songbook/<song>/widgets/), not a shared-tree
// component, so this loads it BY URL with the CAL_WIDGET-style env-var
// override for a source-tree checkout (QS_STAGE precedent — previews
// honour env seams):
//   DOCK_WIDGET=file://$PWD/song/songbook/sonata/widgets/dock.qml \
//     qs -p modules/facets/quickshell/qml/DockPreview.qml
// In the deployed tree ($out/qml/, where songs/ sits beside this file) the
// default relative source resolves with no env needed.
//
// The default source, `songs/sonata/dock.qml`, is sonata's real dock body —
// an ANCHORED PanelWindow hosting every gadget slot as a `WidgetSlot`. That
// body declares `livery`, `bridge`, `shared` and `stagingEngine` as REQUIRED
// properties, so all four must appear in the `createObject` prop map below
// or creation returns null; `shared` is the session-state QtObject the real
// dock is handed by its host, and this harness passes null for it (the
// gadgets that take it as a slot extra guard their own writes).
//
// A REAL `StagingEngine` is instantiated below and threaded through as the
// `stagingEngine` extra — unlike CalendarPreview/ExodosPreview (which hand
// a single already-known widget file straight to `Loader`/`createObject`
// and never touch slot resolution at all), the dock body's whole point is
// to host further `WidgetSlot`s of its own (meters, terminals, …), and
// those need a working `stagingEngine.resolveSong`/`source` to load
// anything. A stub would test nothing about resolution.
ShellRoot {
    id: harness

    property var stubLivery: QtObject {
        property string paletteBg:     "#f2ebde"   // pale marble ground
        property string paletteFg:     "#2f2a33"   // dark plum ink
        property string paletteAccent: "#a07414"   // Attic gold
        property string paletteHot:    "#4e8b45"   // laurel green
        property string paletteUrgent: "#b0472f"   // terracotta
        property string wireCyan:      "#3f867e"   // teal (Doric signature)
        property string holoBlue:      "#345f81"   // aegean
        property string violet:        "#6f4373"   // murex
        property string base09:        "#c06a35"   // clay
        // WidgetSlot.resolvedSong keys off this — the dock's own embedded
        // slots resolve against whichever song this preview stands in for.
        property string songName: "sonata"
    }

    property var stubBridge: QtObject {
        function sendCommand(obj) {
            console.log("[DockPreview] bridge.sendCommand:", JSON.stringify(obj))
        }
    }

    property StagingEngine stagingEngine: StagingEngine {}

    property var dock: null

    Component.onCompleted: {
        var src = Quickshell.env("DOCK_WIDGET")
        if (!src || src === "") src = "songs/sonata/dock.qml"
        var comp = Qt.createComponent(src)
        if (comp.status === Component.Error) {
            console.log("[DockPreview] createComponent failed:", comp.errorString())
            return
        }
        harness.dock = comp.createObject(null, {
            "livery": harness.stubLivery,
            "bridge": harness.stubBridge,
            "shared": null,
            "stagingEngine": harness.stagingEngine
        })
        if (!harness.dock) {
            console.log("[DockPreview] createObject failed for dock body")
            return
        }
    }
}
