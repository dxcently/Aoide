// The fixture powermenu body: no window, only a word on whether its slot
// handed it the shellbridge — which every SurfaceSlot in shell.qml must.
import QtQuick

QtObject {
    id: body
    property var theme
    property var bridge

    function toggle() {}

    Component.onCompleted: console.info("aubade powermenu: bridge is " + (body.bridge ? "set" : "missing"))
}
