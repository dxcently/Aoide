// herald.qml — cadenza's "herald" slot: the THIN window shell of the toast.
//
// Hosted by `SurfaceSlot` (modules/facets/quickshell/qml/slots.md): the root
// is a `PanelWindow` owning its layer surface. Everything drawn — the ledger
// read, the dismiss clock, the blocks, the two commands — lives in
// `HeraldBody.qml`, loaded BY URL (design/kit.md §1); the preview canvas
// refuses a PanelWindow root, so the body is what gets previewed.
//
// Contract (the one sonata's herald keeps):
//   · namespace `aoide-herald`, layer Overlay, no exclusive zone.
//   · Anchored top-right (intent §3.7): the bar's exclusive zone pushes the
//     surface under it; the body supplies the margins in cells.
//   · Mapped only while the body has something to show; sized to the stack.
//   · Keyboard focus None, always: a toast never takes the keyboard (the
//     body's header says why).
import QtQuick
import Quickshell
import Quickshell.Wayland
import "Kit.js" as Kit

PanelWindow {
    id: root

    required property var livery
    required property var bridge

    anchors { top: true; right: true }
    margins {
        top: body.item ? body.item.edgeY : 0
        right: body.item ? body.item.edgeX : 0
    }
    exclusiveZone: 0
    color: "transparent"
    visible: !!body.item && body.item.count > 0
    implicitWidth: Math.max(1, body.item ? body.item.implicitWidth : 1)
    implicitHeight: Math.max(1, body.item ? body.item.implicitHeight : 1)
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "aoide-herald"
    WlrLayershell.keyboardFocus: WlrKeyboardFocus.None

    Loader {
        id: body
        anchors.fill: parent
        Component.onCompleted: setSource(Kit.helper("HeraldBody"), {
            livery: Qt.binding(() => root.livery),
            bridge: Qt.binding(() => root.bridge) })
    }
}
