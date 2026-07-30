import QtQuick
import Quickshell
import Quickshell.Wayland

// Standalone harness to render JUST the Conductor gadget for a screenshot.
//   qs -p modules/facets/quickshell/qml/ConductorPreview.qml
// Floats a ~360x520 overlay surface, centred, carrying a stub palette + bridge.
// Point at a different stage file with QS_STAGE (e.g. an empty one) to exercise
// the empty state:  QS_STAGE=/tmp/empty.json qs -p .../ConductorPreview.qml
ShellRoot {
    PanelWindow {
        id: win
        color: "transparent"
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "aoide-conductor-preview"
        implicitWidth: 360
        implicitHeight: 520

        // faint backdrop so the widget's edge contrast is honestly visible
        Rectangle {
            anchors.fill: parent
            color: "#00000000"
        }

        ConductorGadget {
            anchors.fill: parent
            anchors.margins: 0

            // stub palette — the warm Greek-marble light theme (hex only here).
            // Mirrors the DrachmaState roles the gadget reads, incl. the base16
            // accent spread exposed via noteColor(i) (identity hues per agent).
            notes: QtObject {
                id: stubNotes
                property string paletteBg:     "#e9e2d0"   // pale marble ground
                property string paletteFg:     "#3a2a3a"   // dark plum ink
                property string paletteAccent: "#a07414"   // Attic gold      (base0A)
                property string paletteHot:    "#4e8b45"   // laurel green    (base0B, the one standout)
                property string paletteUrgent: "#b5502f"   // terracotta      (base08)
                property string wireCyan:      "#5f7d6e"   // bronze-verdigris(base0C)
                property string holoBlue:      "#3f6d86"   // aegean          (base0D)
                property string violet:        "#7a4a76"   // murex           (base0E)
                property string glitchPink:    "#b5502f"   // terracotta      (base08)
                property string base09:        "#b3711b"   // amber           (base09)
                property string base0F:        "#8a5a3c"   // rust            (base0F)
                // the 8-hue base16 accent cycle, base08→base0F (DrachmaState order)
                property var accentSpread: [glitchPink, base09, paletteAccent, paletteHot,
                                            wireCyan, holoBlue, violet, base0F]
                function noteColor(id) {
                    var n = accentSpread.length;
                    return accentSpread[((id - 1) % n + n) % n];
                }
            }

            // stub socket sender
            bridge: QtObject {
                function focusSession(id) { console.log("focusSession →", id); }
            }

            // demo cross-widget state: crown one session as traced (laurel green)
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
