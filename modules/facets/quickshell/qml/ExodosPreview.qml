import QtQuick
import Quickshell

// Standalone harness to render the Exodos powermenu for a screenshot.
//   qs -p modules/facets/quickshell/qml/ExodosPreview.qml
// Loads the REAL powermenu.qml pre-shown — it owns its own full-screen
// Overlay PanelWindow (namespace "aoide-powermenu", so a live compositor with
// the facet's layerrules gives this preview the same blur + hyprglass the
// real surface gets). Stub gold-marble palette (hex sanctioned here only —
// mirrors the livery roles, CalendarPreview-style) and a stub bridge that
// logs the power command instead of dispatching it — the preview must never
// actually suspend the box.
//
// The powermenu body is per-song score (song/songbook/<song>/widgets/), NOT
// a shared-tree component, so this harness loads it by URL — the
// CalendarPreview precedent, adapted: powermenu.qml roots a `PanelWindow`,
// not an `Item`, so it can't go through a `Loader` (a Loader parents its
// content visually; a top-level Window has no visual parent slot to fill).
// This uses the SAME `Component.createObject(null, props)` mechanism
// SurfaceSlot.qml uses for the real slot instead. CAL_WIDGET-style env-var
// indirection for a source-tree checkout (QS_STAGE precedent — previews
// honour env seams):
//   POWERMENU_WIDGET=file://$PWD/song/songbook/sonata/widgets/powermenu.qml \
//     qs -p modules/facets/quickshell/qml/ExodosPreview.qml
// In the deployed tree ($out/qml/, where songs/ sits beside this file) the
// default relative source resolves with no env needed.
ShellRoot {
    id: harness

    property var stubNotes: QtObject {
        property string paletteBg:     "#f2ebde"   // pale marble ground
        property string paletteFg:     "#2f2a33"   // dark plum ink
        property string paletteAccent: "#a07414"   // Attic gold (lock)
        property string paletteHot:    "#4e8b45"   // laurel (the chosen stele)
        property string paletteUrgent: "#b0472f"   // terracotta (shutdown)
        property string wireCyan:      "#3f867e"   // verdigris (logout)
        property string holoBlue:      "#345f81"   // aegean (hibernate)
        property string violet:        "#6f4373"   // murex (suspend)
        property string base09:        "#c06a35"   // kiln clay (reboot)
    }

    property var stubBridge: QtObject {
        function sendCommand(obj) {
            console.log("[ExodosPreview] bridge.sendCommand:", JSON.stringify(obj))
        }
    }

    property var exodos: null

    Component.onCompleted: {
        var src = Quickshell.env("POWERMENU_WIDGET")
        if (!src || src === "") src = "songs/sonata/powermenu.qml"
        var comp = Qt.createComponent(src)
        if (comp.status === Component.Error) {
            console.log("[ExodosPreview] createComponent failed:", comp.errorString())
            return
        }
        harness.exodos = comp.createObject(null, {
            "livery": harness.stubNotes,
            "bridge": harness.stubBridge
        })
        if (!harness.exodos) {
            console.log("[ExodosPreview] createObject failed for powermenu.qml")
            return
        }
        // Show on a short delay after load rather than immediately, so the
        // card-deal entrance lands at a predictable wall-clock moment — a
        // screenshot burst can catch the steles mid-swing (the 3D deal), not
        // just the seated rest pose.
        showTimer.start()
    }

    Timer {
        id: showTimer
        interval: 1000
        running: false
        repeat: false
        onTriggered: if (harness.exodos) harness.exodos.show()
    }
}
