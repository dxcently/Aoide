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

    // Depth headroom: the frame's back outline copies offset +depthExtent
    // down-right; the popup surface clips, so the window is that much wider /
    // taller than the frame and the frame is pinned top-left (NOT stretched to
    // the right edge — that would grow the box-math width).
    implicitWidth: popoutWidth + frame.depthExtent
    implicitHeight: frame.implicitHeight + frame.depthExtent + 4  // +4 gap under bar

    GadgetFrame {
        id: frame
        anchors.left: parent.left
        anchors.top: parent.top
        anchors.topMargin: 4
        width: root.popoutWidth
        height: implicitHeight
        notes: root.notes
        title: root.title

        // Glass popout chrome — DECOUPLED from the bar: a CREAM frosted card at
        // 0.72 (the compositor's blur_popups frosts behind it) with plum ink.
        // New law — the strip is air (0.30) but every floating pane joins the
        // frame/dock glass tier at 0.72, so the now-playing / volume / battery /
        // calendar popouts read SOLID over arbitrary windows and carry their
        // dense text. (The dock's panes leave these unset.)
        glassColor: Qt.rgba(Qt.color(root.notes.paletteBg).r,
                            Qt.color(root.notes.paletteBg).g,
                            Qt.color(root.notes.paletteBg).b, 0.72)
        glassOpacity: 1.0
        outlineColor: root.notes.paletteFg
        depthColor: root.notes.paletteFg
        labelColor: root.notes.paletteFg

        Item {
            id: slot
            width: parent.width
            implicitHeight: childrenRect.height
        }
    }
}
