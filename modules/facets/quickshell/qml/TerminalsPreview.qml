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

                // context-meter helpers — the gadget calls notes.ctx* (LiveryState
                // owns the real ones); mirrored here so the stub renders the meter
                // instead of throwing. Same math, verbatim.
                function ctxCeiling(ceiling) { return (ceiling && ceiling > 0) ? ceiling : 200000; }
                function ctxPercent(tokens, ceiling) {
                    var t = tokens || 0;
                    if (t <= 0) return 0;
                    var pct = t / ctxCeiling(ceiling) * 100;
                    return pct < 0 ? 0 : (pct > 100 ? 100 : pct);
                }
                function ctxCompact(n) {
                    var v = n || 0;
                    if (v < 1000) return String(v);
                    if (v < 1000000) return Math.round(v / 1000) + "k";
                    return (v / 1000000).toFixed(1) + "M";
                }
                function ctxBar(pct, cells) {
                    var n = cells || 8;
                    var filled = Math.round(Math.max(0, Math.min(100, pct)) / 100 * n);
                    var s = "[";
                    for (var i = 0; i < n; i++) s += (i < filled) ? "▓" : "░";
                    return s + "]";
                }
                function ctxColor(pct, accent) { return pct >= 85 ? paletteUrgent : accent; }
                // per-workspace note hue — mirrors LiveryState.noteColor (the
                // accentSpread cycle; the kimi magic/scratch wsTag work calls
                // this). Same fallback shape: out-of-range ids → paletteAccent.
                function noteColor(id) {
                    var n = parseInt(id, 10);
                    if (isNaN(n) || n <= 0) return paletteAccent;
                    var spread = [violet, holoBlue, wireCyan, paletteHot, paletteAccent];
                    return spread[(n - 1) % spread.length];
                }
            }

            // stub socket sender
            bridge: QtObject {
                function focusSession(id) { console.log("focusSession →", id); }
            }

            // demo cross-widget state: crown one terminal as traced (laurel green).
            // hover* mirror shell.qml's shared object (same trio as
            // ConductorPreview's stub) so a pointer crossing the overlay
            // doesn't spray non-existent-property warnings.
            shared: QtObject {
                property string tracedSessionId: "694058a0-ae90-46a4-8457-dd1a235f5dd7"
                property string hoveredSessionId: ""
                property int hoveredWorkspace: -1
            }

            stagePath: {
                var e = Quickshell.env("QS_STAGE");
                return (e && e.length > 0) ? e
                                           : "/home/khoa/Aoide/song/stage/sessions.json";
            }
        }
    }
}
