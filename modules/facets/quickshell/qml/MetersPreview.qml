import QtQuick
import Quickshell
import Quickshell.Wayland

// Standalone harness to render the sonata meters temple for a screenshot.
//   qs -p modules/facets/quickshell/qml/MetersPreview.qml
// Floats a ~360x300 overlay surface carrying a stub gold-marble palette; the
// widget reads the machine's real /proc/stat + /proc/meminfo live.
//
// The meters body is per-song score (song/songbook/<song>/widgets/), NOT a
// shared-tree component, so this harness loads it by URL — the
// CalendarPreview precedent. Quickshell's virtual qs:@ filesystem black-holes
// relative paths that escape the config root, so from a repo checkout point
// METERS_WIDGET at the songbook file (QS_STAGE precedent — previews honour
// env seams):
//   METERS_WIDGET=file://$PWD/song/songbook/sonata/widgets/meters.qml \
//     qs -p modules/facets/quickshell/qml/MetersPreview.qml
// In the deployed tree ($out/qml/, where songs/ sits beside this file) the
// default relative source resolves with no env needed.
//
// meters.qml declares `livery` AND `bridge` as `required property` (the
// facet original only requires `livery`) — a plain `Loader { source: }`
// cannot satisfy a required property (creation fails, null item, the
// DockPreview Phase-3 hazard), so this goes through `setSource(url, props)`
// with BOTH supplied.
ShellRoot {
    PanelWindow {
        id: win
        color: "transparent"
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "aoide-meters-preview"
        implicitWidth: 360
        implicitHeight: 300

        Rectangle {
            anchors.fill: parent
            color: "#00000000"
        }

        property var stubLivery: QtObject {
            property string paletteBg:     "#f2ebde"   // pale marble ground
            property string paletteFg:     "#2f2a33"   // dark plum ink
            property string paletteAccent: "#a07414"   // Attic gold
            property string paletteHot:    "#4e8b45"   // laurel green (the one standout)
            property string paletteUrgent: "#b0472f"   // terracotta
            property string wireCyan:      "#3f867e"   // teal (Doric signature)
            property string holoBlue:      "#345f81"   // aegean (RAM fill)
            property string violet:        "#6f4373"   // murex (CPU fill)
        }

        // stub socket sender — meters.qml declares it per the universal
        // widget contract (slots.md) but never calls it
        property var stubBridge: QtObject {
            function sendCommand(obj) {
                console.log("[MetersPreview] bridge.sendCommand:", JSON.stringify(obj))
            }
        }

        // NOT anchors.fill — an explicit Loader size forces the loaded item
        // to fill it, overriding the item's own implicitWidth/implicitHeight
        // (WidgetSlot.qml sizes ITSELF off the loaded item instead; see its
        // own header note). Position only, so a missing/zero implicit size
        // in meters.qml would actually show up here rather than being
        // silently stretched to fill.
        Loader {
            anchors.top: parent.top
            anchors.left: parent.left
            Component.onCompleted: {
                var src = Quickshell.env("METERS_WIDGET")
                if (!src || src === "") src = "songs/sonata/meters.qml"
                setSource(src, { "livery": win.stubLivery, "bridge": win.stubBridge })
            }
        }
    }
}
