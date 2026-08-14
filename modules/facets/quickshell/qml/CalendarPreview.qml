import QtQuick
import Quickshell
import Quickshell.Wayland

// Standalone harness to render the sonata calendar scroll for a screenshot.
//   qs -p modules/facets/quickshell/qml/CalendarPreview.qml
// Floats the widget on a transparent overlay surface (matching the real
// StelePopout host) with the stub gold-marble palette (hex sanctioned here
// only — mirrors the livery roles, AudioColonnadePreview-style) and a null
// bridge (calendar declares it per the slot contract but never uses it). The
// widget frames itself (a
// self-framed papyrus stele) — no GadgetFrame wrapper here, matching its
// StelePopout host. The unfurl reveal plays once when the surface maps.
//
// The calendar body is per-song score (song/songbook/<song>/widgets/), NOT a
// shared-tree component, so this harness loads it by URL. Quickshell's
// virtual qs:@ filesystem black-holes relative paths that escape the config
// root, so from a repo checkout point CAL_WIDGET at the songbook file
// (QS_STAGE precedent — previews honour env seams):
//   CAL_WIDGET=file://$PWD/song/songbook/sonata/widgets/calendar.qml \
//     qs -p modules/facets/quickshell/qml/CalendarPreview.qml
// In the deployed tree ($out/qml/, where songs/ sits beside this file) the
// default relative source resolves with no env needed.
ShellRoot {
    PanelWindow {
        id: win
        color: "transparent"             // real StelePopout hosts are transparent too
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "aoide-calendar-preview"
        anchors { top: true; left: true }
        implicitWidth: 460
        implicitHeight: 640

        property var stub: QtObject {
            property string paletteBg:     "#f2ebde"   // pale marble ground
            property string paletteFg:     "#2f2a33"   // dark plum ink
            property string paletteAccent: "#a07414"   // Attic gold (year, tallies, 𝄂)
            property string paletteHot:    "#4e8b45"   // laurel (today's cell)
            property string paletteUrgent: "#b0472f"   // terracotta
            property string wireCyan:      "#3f867e"   // teal
            property string holoBlue:      "#345f81"   // aegean
            property string violet:        "#6f4373"   // murex
            property string base09:        "#c06a35"   // clay — the rubric signature
        }

        Loader {
            // top-anchored like the real popup (it grows downward on the
            // compact↔expanded toggle, matching the bar-hung physics)
            anchors.top: parent.top
            anchors.topMargin: 16
            anchors.horizontalCenter: parent.horizontalCenter
            Component.onCompleted: {
                var src = Quickshell.env("CAL_WIDGET")
                if (!src || src === "") src = "songs/sonata/calendar.qml"
                setSource(src, { "notes": win.stub, "bridge": null })
            }
        }
    }
}
