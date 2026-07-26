// WorkspaceRow.qml — workspace switcher strip (skeleton).
//
// Stub: renders numbered workspace buttons. Will bind to Hyprland workspace
// state via stage/workspaces.json (shellbridge emits this). Clicking a button
// calls bridge.sendCommand({ cmd: "workspace", id: n }).

import QtQuick
import QtQuick.Layouts

Row {
    id: root
    required property var notes
    spacing: 4

    // STUB: static 1-5 until shellbridge emits stage/workspaces.json
    Repeater {
        model: 5
        delegate: Rectangle {
            width: 24
            height: 24
            radius: 4
            color: index === 0 ? notes.barAccent : "transparent"
            border.color: notes.barAccent
            border.width: 1
            Text {
                anchors.centerIn: parent
                text: index + 1
                color: index === 0 ? notes.barBg : notes.barFg
                font.pixelSize: 11
            }
        }
    }
}
