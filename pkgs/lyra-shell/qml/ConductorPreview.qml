import QtQuick
import Quickshell
import Quickshell.Wayland

// Standalone harness to render the sonata conductor temple for a screenshot.
//   qs -p modules/facets/quickshell/qml/ConductorPreview.qml
// Floats a 360x520 overlay surface. Unlike MetersPreview/PowerPreview's stub
// palette, the roster needs LiveryState's real helpers (ctxPercent/ctxBar/
// noteColor/elapsedSince), so the harness instantiates the real livery +
// bridge and a stub `shared`. RosterTemple honours QS_STAGE, so a fixture
// stage dir can be pointed at to exercise empty/multi-project states:
//   QS_STAGE=/tmp/stage-fixture qs -p …/ConductorPreview.qml
//
// The conductor body is per-song score (song/songbook/<song>/widgets/), NOT
// a shared-tree component, so this harness loads it by URL — the
// CalendarPreview precedent. Quickshell's virtual qs:@ filesystem
// black-holes relative paths that escape the config root, so from a repo
// checkout point CONDUCTOR_WIDGET at the songbook file (QS_STAGE
// precedent — previews honour env seams):
//   CONDUCTOR_WIDGET=file://$PWD/song/songbook/sonata/widgets/conductor.qml \
//     qs -p modules/facets/quickshell/qml/ConductorPreview.qml
// In the deployed tree ($out/qml/, where songs/ sits beside this file) the
// default relative source resolves with no env needed.
//
// conductor.qml declares `livery` AND `bridge` as `required property`
// (matching the facet original); `shared` is a plain (non-required)
// property, defaulting to null — dock.qml's own anchor still passes it as a
// slot extra (`extraProps: ({ shared: root.shared })`), so this harness
// keeps supplying its stub for parity. A plain `Loader { source: }` cannot
// satisfy a required property (creation fails, null item, the DockPreview
// Phase-3 hazard), so this goes through `setSource(url, props)`.
ShellRoot {
    PanelWindow {
        id: win
        color: "transparent"
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.layer: WlrLayer.Overlay
        WlrLayershell.namespace: "aoide-conductor-preview"
        implicitWidth: 360
        implicitHeight: 520

        LiveryState { id: livery }
        ShellBridge { id: bridge }
        QtObject {
            id: sharedStub
            property string tracedSessionId: ""
            property int hoveredWorkspace: -1
            property string hoveredSessionId: ""
        }

        // NOT anchors.fill — an explicit Loader size forces the loaded item
        // to fill it, overriding the item's own implicitWidth/implicitHeight
        // (WidgetSlot.qml sizes ITSELF off the loaded item instead; see its
        // own header note). Position only, so a missing/zero implicit size
        // in conductor.qml would actually show up here rather than being
        // silently stretched to fill.
        Loader {
            anchors.top: parent.top
            anchors.left: parent.left
            Component.onCompleted: {
                var src = Quickshell.env("CONDUCTOR_WIDGET")
                if (!src || src === "") src = "songs/sonata/conductor.qml"
                setSource(src, { "livery": livery, "bridge": bridge, "shared": sharedStub })
            }
        }
    }
}
