import QtQuick
import Quickshell
import Quickshell.Wayland

// Standalone harness to render the audio colonnade stele for a screenshot.
//   qs -p modules/facets/quickshell/qml/AudioColonnadePreview.qml
// Pins an opaque marble surface top-left carrying the stub gold-marble palette,
// and shows the widget TWICE at real size: a live state (both columns lit) and a
// muted state (a BROKEN/ruined column) — so the two orders, the mic-vs-vol read,
// and the broken-column silhouette are all visible. The widget frames itself
// (a self-framed stele) — no GadgetFrame wrapper here.
ShellRoot {
    PanelWindow {
        id: win
        color: "#cfc6b1"                 // a mid-marble backdrop (screenshot only)
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "aoide-audio-preview"
        anchors { top: true; left: true }
        implicitWidth: 800        // two 352-wide tristyle steles + the gap
        implicitHeight: 440

        property var stub: QtObject {
            property string paletteBg:     "#f2ebde"
            property string paletteFg:     "#2f2a33"
            property string paletteAccent: "#a07414"   // Attic gold  — VOL fill
            property string paletteHot:    "#4e8b45"   // laurel
            property string paletteUrgent: "#b0472f"   // terracotta  — the wound / MUTED
            property string wireCyan:      "#3f867e"   // teal
            property string holoBlue:      "#345f81"   // aegean      — MIC fill
            property string violet:        "#6f4373"   // murex
            property string base09:        "#c06a35"   // amber       — the stele's signature
        }

        // Fabricated rosters — this harness has no PipeWire or bluez behind
        // it. They exist so the plinth chips have a name to carry and the
        // picker has rows to draw; nothing here was read off a service.
        readonly property var sinks: [
            { key: "1", name: "SN6186 Analog", gloss: "analog-stereo", current: true },
            { key: "2", name: "24G1WG4",       gloss: "hdmi-stereo",   current: false },
            { key: "3", name: "WH-1000XM4",    gloss: "bluez-output",  current: false }
        ]
        readonly property var sources: [
            { key: "4", name: "SN6186 Analog", gloss: "analog-stereo", current: true }
        ]
        readonly property var devices: [
            { key: "a", name: "WH-1000XM4",  gloss: "connected", current: true },
            { key: "b", name: "MX Master 3", gloss: "paired",    current: false },
            { key: "c", name: "Pixel Buds",  gloss: "seen",      current: false }
        ]

        Row {
            anchors.centerIn: parent
            spacing: 34

            AudioColonnade {                          // live: vol 86 · mic 100
                livery: win.stub                       // · bt bound in A2DP
                outPct: 86;  outMuted: false; outAvail: true
                inPct: 100;  inMuted: false; inAvail: true
                btAvail: true; btOn: true; btConnected: true
                btName: "WH-1000XM4"; btProfile: "a2dp"
                sinkRoster: win.sinks
                sourceRoster: win.sources
                btRoster: win.devices
            }
            AudioColonnade {                          // the BT bay OPEN, with a
                livery: win.stub                       // fabricated device roster
                outPct: 40;  outMuted: false; outAvail: true
                inPct: 65;   inMuted: true;  inAvail: true
                btAvail: true; btOn: true; btConnected: true
                btName: "WH-1000XM4"; btProfile: "a2dp"
                sinkRoster: win.sinks
                sourceRoster: win.sources
                btRoster: win.devices
                openBay: 2
            }
        }
    }
}
