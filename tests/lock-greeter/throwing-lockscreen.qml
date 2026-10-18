// A lock face whose start-up throws: it compiles and is created, paints
// a field-less screen, and never sets `auth.ready` — the host must destroy
// it and show the floor.
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
