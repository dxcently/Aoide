import QtQuick
import Quickshell
import Quickshell.Wayland

// Standalone harness to render JUST the Terminals gadget for a screenshot.
//   qs -p modules/facets/quickshell/qml/TerminalsPreview.qml
// Floats a ~360x520 overlay surface, centred, carrying a stub palette + bridge.
// Point at a different stage file with QS_STAGE (e.g. an empty one) to exercise
// the empty state:  QS_STAGE=/tmp/empty.json qs -p .../TerminalsPreview.qml
ShellRoot {
    PanelWindow {
        id: win
        color: "transparent"
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "aoide-terminals-preview"
        implicitWidth: 360
        implicitHeight: 520

        // faint backdrop so the widget's edge contrast is honestly visible
        Rectangle {
            anchors.fill: parent
            color: "#00000000"
        }

        TerminalsGadget {
            anchors.fill: parent
            anchors.margins: 0

            // stub palette — the warm gold-marble light theme (hex only here)
            notes: QtObject {
                property string paletteBg:     "#f2ebde"   // pale marble ground
                property string paletteFg:     "#2f2a33"   // dark plum ink
                property string paletteAccent: "#a07414"   // Attic gold
                property string paletteHot:    "#4e8b45"   // laurel green (the one standout)
                property string paletteUrgent: "#b0472f"   // terracotta
                property string wireCyan:      "#3f867e"   // bronze-verdigris
                property string holoBlue:      "#345f81"   // aegean
                property string violet:        "#6f4373"   // murex
            }

            // stub socket sender
            bridge: QtObject {
                function focusSession(id) { console.log("focusSession →", id); }
            }

            // demo cross-widget state: crown one terminal as traced (laurel green)
            shared: QtObject {
                property string tracedSessionId: "694058a0-ae90-46a4-8457-dd1a235f5dd7"
            }

            stagePath: {
                var e = Quickshell.env("QS_STAGE");
                return (e && e.length > 0) ? e
                                           : "/home/khoa/Aoide/song/stage/sessions.json";
            }
        }
    }
}
