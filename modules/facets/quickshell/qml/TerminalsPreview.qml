import QtQuick
import Quickshell
import Quickshell.Wayland

// Standalone harness to render the sonata terminals temple for a screenshot.
//   qs -p modules/facets/quickshell/qml/TerminalsPreview.qml
// Floats a ~360x520 overlay surface, centred, carrying a stub palette + bridge.
// Point at a different stage file with QS_STAGE (e.g. an empty one) to exercise
// the empty state:  QS_STAGE=/tmp/empty.json qs -p .../TerminalsPreview.qml
//
// The terminals body is per-song score (song/songbook/<song>/widgets/), NOT
// a shared-tree component, so this harness loads it by URL — the
// CalendarPreview precedent. Quickshell's virtual qs:@ filesystem
// black-holes relative paths that escape the config root, so from a repo
// checkout point TERMINALS_WIDGET at the songbook file (QS_STAGE
// precedent — previews honour env seams):
//   TERMINALS_WIDGET=file://$PWD/song/songbook/sonata/widgets/terminals.qml \
//     qs -p modules/facets/quickshell/qml/TerminalsPreview.qml
// In the deployed tree ($out/qml/, where songs/ sits beside this file) the
// default relative source resolves with no env needed.
//
// terminals.qml declares `livery` AND `bridge` as `required property`
// (matching the facet original); `shared` and `stagePath` are plain
// (non-required) properties with their own defaults — dock.qml's own anchor
// still passes `shared` as a slot extra (`extraProps: ({ shared: root.shared
// })`), and this harness keeps overriding both for parity with the direct-
// instantiation version. A plain `Loader { source: }` cannot satisfy a
// required property (creation fails, null item, the DockPreview Phase-3
// hazard), so this goes through `setSource(url, props)` with every property
// below in one initial-properties object.
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

        // stub palette — the warm gold-marble light theme (hex only here)
        property var stubLivery: QtObject {
            property string paletteBg:     "#f2ebde"   // pale marble ground
            property string paletteFg:     "#2f2a33"   // dark plum ink
            property string paletteAccent: "#a07414"   // Attic gold
            property string paletteHot:    "#4e8b45"   // laurel green (the one standout)
            property string paletteUrgent: "#b0472f"   // terracotta
            property string wireCyan:      "#3f867e"   // bronze-verdigris
            property string holoBlue:      "#345f81"   // aegean
            property string violet:        "#6f4373"   // murex

            // context-meter helpers — the widget calls livery.ctx* (LiveryState
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
        property var stubBridge: QtObject {
            function focusSession(id) { console.log("focusSession →", id); }
        }

        // demo cross-widget state: crown one terminal as traced (laurel green).
        // hover* mirror shell.qml's shared object (same trio as
        // ConductorPreview's stub) so a pointer crossing the overlay
        // doesn't spray non-existent-property warnings.
        property var stubShared: QtObject {
            property string tracedSessionId: "694058a0-ae90-46a4-8457-dd1a235f5dd7"
            property string hoveredSessionId: ""
            property int hoveredWorkspace: -1
        }

        property string stubStagePath: {
            var e = Quickshell.env("QS_STAGE");
            return (e && e.length > 0) ? e
                                       : (Quickshell.env("HOME") + "/Aoide/song/stage/sessions.json");
        }

        // NOT anchors.fill — an explicit Loader size forces the loaded item
        // to fill it, overriding the item's own implicitWidth/implicitHeight
        // (WidgetSlot.qml sizes ITSELF off the loaded item instead; see its
        // own header note). Position only, so a missing/zero implicit size
        // in terminals.qml would actually show up here rather than being
        // silently stretched to fill.
        Loader {
            anchors.top: parent.top
            anchors.left: parent.left
            Component.onCompleted: {
                var src = Quickshell.env("TERMINALS_WIDGET")
                if (!src || src === "") src = "songs/sonata/terminals.qml"
                setSource(src, {
                    "livery": win.stubLivery,
                    "bridge": win.stubBridge,
                    "shared": win.stubShared,
                    "stagePath": win.stubStagePath
                })
            }
        }
    }
}
