// BarPopout.qml — one bar-spawned gadget popout.
//
// A real PopupWindow (xdg_popup child of the bar's PanelWindow) hanging under
// a bar cell — NOT an in-Item Rectangle: the bar surface is 28px tall, so
// anything drawn below it inside the bar Item is clipped by the layer surface
// and never renders (the v1 popouts' silent failure). A popup is its own
// surface, free to extend past the strip.
//
// Chrome: the same GadgetFrame ASCII box the dock gadgets wear (╔═[ TITLE ]═╗,
// glass over compositor blur — the compositor facet's blur_popups layerrule
// extends the aoide-bar blur to these popups). All colors from notes.
//
// Usage (children land in the frame body, GadgetFrame-style):
//   BarPopout {
//       notes: root.notes; cell: clockText; title: "CALENDAR"
//       shown: root.calShown
//       CalendarGadget { width: parent.width; notes: root.notes }
//   }

import QtQuick
import Quickshell

PopupWindow {
    id: root

    required property var notes
    required property Item cell    // the bar cell this popout hangs under
    property string title: ""
    property int popoutWidth: 288
    property bool shown: false

    // Children forward into the frame's body via an explicit slot Item —
    // aliasing GadgetFrame's own default alias (alias-to-alias) is invalid QML.
    default property alias content: slot.data

    anchor.item: cell
    anchor.edges: Edges.Bottom
    anchor.gravity: Edges.Bottom

    visible: shown
    color: "transparent"

    implicitWidth: popoutWidth
    implicitHeight: frame.implicitHeight + 4   // +4 → breathing gap under the bar

    GadgetFrame {
        id: frame
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.topMargin: 4
        height: implicitHeight
        notes: root.notes
        title: root.title

        Item {
            id: slot
            width: parent.width
            implicitHeight: childrenRect.height
        }
    }
}
