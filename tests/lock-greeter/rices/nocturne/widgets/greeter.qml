// A greeter face whose start-up throws: it is created, paints a field-less
// screen and never sets `auth.ready` — the greeter host must destroy it and
// show its floor.
import QtQuick

Rectangle {
    id: face
    property var theme
    property var bridge
    property var auth

    color: "#ff00ff"

    Component.onCompleted: {
        throw new Error("the face's start-up failed")
        face.auth.ready = true
    }
}
