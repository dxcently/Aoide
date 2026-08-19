import QtQuick
import Quickshell
import Quickshell.Wayland

// Standalone harness to render JUST the Conductor gadget for a screenshot.
//   qs -p modules/facets/quickshell/qml/ConductorPreview.qml
// Floats a 360x520 overlay surface. Unlike MetersPreview's stub palette, the
// roster needs LiveryState's real helpers (ctxPercent/ctxBar/noteColor/
// elapsedSince), so the harness instantiates the real livery + bridge and a
// stub `shared`. RosterTemple honours QS_STAGE, so a fixture stage dir can be
// pointed at to exercise empty/multi-project states:
//   QS_STAGE=/tmp/stage-fixture qs -p …/ConductorPreview.qml
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

        ConductorGadget {
            anchors.fill: parent
            livery: livery
            bridge: bridge
            shared: sharedStub
        }
    }
}
