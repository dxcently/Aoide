// WorkspaceRow.qml — workspace switcher strip (skeleton).
//
// Stub: renders numbered workspace buttons. Will bind to Hyprland workspace
// state via stage/workspaces.json (shellbridge emits this). Clicking a button
// calls bridge.sendCommand({ cmd: "workspace", id: n }).

import QtQuick
import QtQuick.Layouts

Row {
    id: root
    required property var tokens
    spacing: 4

    // STUB: static 1-5 until shellbridge emits stage/workspaces.json
    Repeater {
        model: 5
        delegate: Rectangle {
            width: 24
            height: 24
            radius: 4
            color: index === 0 ? tokens.barAccent : "transparent"
            border.color: tokens.barAccent
            border.width: 1
            Text {
                anchors.centerIn: parent
                text: index + 1
                color: index === 0 ? tokens.barBg : tokens.barFg
                font.pixelSize: 11
            }
        }
    }
}
