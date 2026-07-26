// AoideGreeter.qml — login greeter (skeleton).
//
// Quickshell-native greeter. Presented before the user session starts.
// Uses the greetd IPC protocol. Colors from tokens; wallpaper underneath.

import QtQuick
import QtQuick.Layouts

Item {
    id: root

    required property var tokens

    // ── Greeter UI ─────────────────────────────────────────────────────────
    Rectangle {
        anchors.fill: parent
        color: Qt.rgba(
            parseInt(tokens.paletteBg.slice(1,3), 16) / 255,
            parseInt(tokens.paletteBg.slice(3,5), 16) / 255,
            parseInt(tokens.paletteBg.slice(5,7), 16) / 255,
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
                color: tokens.paletteAccent

                Text {
                    anchors.centerIn: parent
                    text: "K"  // STUB: user initial
                    color: tokens.paletteBg
                    font.pixelSize: 32
                    font.bold: true
                }
            }

            // Username label
            Text {
                Layout.alignment: Qt.AlignHCenter
                text: "khoa"  // STUB: from greetd session info
                color: tokens.paletteFg
                font.pixelSize: 20
            }

            // Password input
            Rectangle {
                Layout.fillWidth: true
                height: 44
                radius: 8
                color: tokens.barBg
                border.color: tokens.paletteAccent
                border.width: 1

                TextInput {
                    anchors { fill: parent; margins: 10 }
                    echoMode: TextInput.Password
                    color: tokens.paletteFg
                    font.pixelSize: 15
                    placeholderText: "Password"
                    // STUB: onAccepted → greetd create_session + start_session
                }
            }

            // Session selector (STUB)
            Text {
                Layout.alignment: Qt.AlignHCenter
                text: "Hyprland"
                color: tokens.barAccent
                font.pixelSize: 12
            }
        }
    }
}
