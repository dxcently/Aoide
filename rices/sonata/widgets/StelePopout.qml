// StelePopout.qml — a bar-spawned popout that hosts a SELF-FRAMED stele.
//
// Sibling of BarPopout, but for the pantheon's marble-stele widgets (the grammar
// ConductorGadget/TerminalsGadget/NotificationCard wear, and now AudioColonnade):
// the child draws ALL its own chrome — opaque marble body, 2px border, inset
// keyline, cast shadow, TUI frame — so this host adds NO GadgetFrame entablature
// (that would be chrome-on-chrome). It is just the real PopupWindow (an xdg_popup
// child of the bar's PanelWindow) hanging under a bar cell, sized to its child.
//
// khoa, 2026-07-31: standing direction — new bar widgets converge their outer
// panel on the stele family, hosted bare through this component, not GadgetFrame.
//
// Usage (the child is a stele that sets its own width + implicitHeight):
//   StelePopout {
//       cell: volText; shown: root.audioShown
//       AudioColonnade { livery: root.livery; /* … */ }
//   }

import QtQuick
import Quickshell

PopupWindow {
    id: root

    required property Item cell        // the bar cell this popout hangs under
    property bool shown: false
    default property alias content: host.data

    // Anchor override (additive; defaults preserve the centered hang all
    // existing hosts get). A host whose stele RESIZES while open — the
    // calendar's compact↔expanded scroll morph is the first — pins the
    // popup's left edge (edges Bottom|Left, gravity Bottom|Right) instead:
    // a centered xdg_popup is re-centered by the compositor on every width
    // change, one frame behind the resize, which visibly twitches the popup
    // mid-morph (live-verified); an edge-pinned popup grows away from its
    // fixed edge with no repositioning at all.
    property int anchorEdges: Edges.Bottom
    property int anchorGravity: Edges.Bottom

    anchor.item: cell
    anchor.edges: anchorEdges
    anchor.gravity: anchorGravity

    visible: shown
    color: "transparent"

    // Sized to the hosted stele (its first child), plus room for the stele's own
    // cast-shadow overhang (4px right / the child's implicitHeight already carries
    // the 5px bottom) and a 4px gap under the bar strip.
    readonly property Item body: host.children.length > 0 ? host.children[0] : null
    implicitWidth: (body ? body.width : 260) + 6
    implicitHeight: (body ? body.implicitHeight : 200) + 8

    Item {
        id: host
        anchors.left: parent.left
        anchors.top: parent.top
        anchors.topMargin: 4
        width: root.body ? root.body.width : parent.width
        implicitHeight: root.body ? root.body.implicitHeight : 0
    }
}
