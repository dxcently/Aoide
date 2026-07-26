// AoideGreeter.qml — login greeter (skeleton).
//
// Quickshell-native greeter. Presented before the user session starts.
// Uses the greetd IPC protocol. Colors from notes; wallpaper underneath.

import QtQuick
import QtQuick.Layouts

Item {
    id: root

    required property var notes

    // ── Greeter UI ─────────────────────────────────────────────────────────
    Rectangle {
        anchors.fill: parent
        color: Qt.rgba(
            parseInt(notes.paletteBg.slice(1,3), 16) / 255,
            parseInt(notes.paletteBg.slice(3,5), 16) / 255,
            parseInt(notes.paletteBg.slice(5,7), 16) / 255,
            0.90
        )

        ColumnLayout {
            anchors.centerIn: parent
            spacing: 20
            width: 320

            // User avatar placeholder
            Rectangle {
                Layout.alignment: Qt.AlignHCenter
                width: 80; height: 80
                radius: 40
                color: notes.paletteAccent

                Text {
                    anchors.centerIn: parent
                    text: "K"  // STUB: user initial
                    color: notes.paletteBg
                    font.pixelSize: 32
                    font.bold: true
                }
            }

            // Username label
            Text {
                Layout.alignment: Qt.AlignHCenter
                text: "khoa"  // STUB: from greetd session info
                color: notes.paletteFg
                font.pixelSize: 20
            }

            // Password input
            Rectangle {
                Layout.fillWidth: true
                height: 44
                radius: 8
                color: notes.barBg
                border.color: notes.paletteAccent
                border.width: 1

                TextInput {
                    anchors { fill: parent; margins: 10 }
                    echoMode: TextInput.Password
                    color: notes.paletteFg
                    font.pixelSize: 15
                    placeholderText: "Password"
                    // STUB: onAccepted → greetd create_session + start_session
                }
            }

            // Session selector (STUB)
            Text {
                Layout.alignment: Qt.AlignHCenter
                text: "Hyprland"
                color: notes.barAccent
                font.pixelSize: 12
            }
        }
    }
}
