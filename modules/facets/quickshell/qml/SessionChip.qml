// SessionChip.qml — agent session indicator chip (Terminal-Commander widget).
//
// One chip per live agent session. Clicking it triggers session-jump:
//   chip click → onClicked signal → AoideBar calls bridge.focusSession(address)
//   → ShellBridge sends to shellbridge socket
//   → shellbridge runs: hyprctl dispatch focuswindow address:<addr>
//
// This is the session-jump stub (concepts/Desktop-Architecture §Session jump flow).

import QtQuick

Rectangle {
    id: root

    required property var notes
    required property var sessionData  // { id, name, address, state }

    signal clicked

    implicitWidth: label.implicitWidth + 16
    implicitHeight: 22
    radius: 4
    color: sessionData && sessionData.state === "active"
           ? notes.paletteAccent
           : notes.paletteBg
    border.color: notes.barAccent
    border.width: 1

    Text {
        id: label
        anchors.centerIn: parent
        text: sessionData ? sessionData.name : "?"
        color: sessionData && sessionData.state === "active"
               ? notes.barBg
               : notes.barFg
        font.pixelSize: 11
    }

    MouseArea {
        anchors.fill: parent
        onClicked: root.clicked()
        cursorShape: Qt.PointingHandCursor
    }
}
