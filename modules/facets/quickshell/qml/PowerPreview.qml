import QtQuick
import Quickshell
import Quickshell.Wayland

// Standalone harness to render the sonata power temple for a screenshot.
//   qs -p modules/facets/quickshell/qml/PowerPreview.qml
// Floats a ~340x300 overlay surface carrying a stub gold-marble palette; the
// widget reads real UPower (battery) + /proc/net/route (network) live. On a
// desktop with no battery it exercises the honest "AC — no battery" path.
//
// The power body is per-song score (song/songbook/<song>/widgets/), NOT a
// shared-tree component, so this harness loads it by URL — the
// CalendarPreview precedent. Quickshell's virtual qs:@ filesystem black-holes
// relative paths that escape the config root, so from a repo checkout point
// POWER_WIDGET at the songbook file (QS_STAGE precedent — previews honour env
// seams):
//   POWER_WIDGET=file://$PWD/song/songbook/sonata/widgets/power.qml \
//     qs -p modules/facets/quickshell/qml/PowerPreview.qml
// In the deployed tree ($out/qml/, where songs/ sits beside this file) the
// default relative source resolves with no env needed.
//
// power.qml declares `livery` AND `bridge` as `required property` (the
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
        WlrLayershell.namespace: "aoide-power-preview"
        implicitWidth: 340
        implicitHeight: 300

        Rectangle {
            anchors.fill: parent
            color: "#00000000"
        }

        property var stubLivery: QtObject {
            property string paletteBg:     "#f2ebde"   // pale marble ground
            property string paletteFg:     "#2f2a33"   // dark plum ink
            property string paletteAccent: "#a07414"   // Attic gold (numeric tallies)
            property string paletteHot:    "#4e8b45"   // laurel green (live-link standout)
            property string paletteUrgent: "#b0472f"   // terracotta (low / offline)
            property string wireCyan:      "#3f867e"   // teal (charge fill)
            property string holoBlue:      "#345f81"   // aegean (link + detail)
            property string violet:        "#6f4373"   // murex (Corinthian signature)
        }

        // stub socket sender — power.qml declares it per the universal
        // widget contract (slots.md) but never calls it
        property var stubBridge: QtObject {
            function sendCommand(obj) {
                console.log("[PowerPreview] bridge.sendCommand:", JSON.stringify(obj))
            }
        }

        // NOT anchors.fill — an explicit Loader size forces the loaded item
        // to fill it, overriding the item's own implicitWidth/implicitHeight
        // (WidgetSlot.qml sizes ITSELF off the loaded item instead; see its
        // own header note). Position only, so a missing/zero implicit size
        // in power.qml would actually show up here rather than being
        // silently stretched to fill.
        Loader {
            anchors.top: parent.top
            anchors.left: parent.left
            Component.onCompleted: {
                var src = Quickshell.env("POWER_WIDGET")
                if (!src || src === "") src = "songs/sonata/power.qml"
                setSource(src, { "livery": win.stubLivery, "bridge": win.stubBridge })
            }
        }
    }
}
