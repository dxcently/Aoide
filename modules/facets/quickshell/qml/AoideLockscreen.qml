// AoideLockscreen.qml — lockscreen (skeleton).
//
// Quickshell-native lockscreen — replaces hyprlock. Implements the
// ext-session-lock-v1 Wayland protocol to take exclusive compositor lock.
// Colors from notes; wallpaper surface sits beneath via AoideWallpaper.

import QtQuick
import QtQuick.Layouts
import Quickshell.Wayland

Item {
    id: root

    required property var notes

    // ── Session lock (Wayland ext-session-lock-v1) ─────────────────────────
    // STUB: SessionLock is the Quickshell type for ext-session-lock-v1.
    // When activated it takes the compositor lock and presents this surface.

    // ── Lock UI ────────────────────────────────────────────────────────────
    Rectangle {
        anchors.fill: parent
        // Semi-transparent overlay on top of wallpaper
        color: Qt.rgba(
            parseInt(notes.paletteBg.slice(1,3), 16) / 255,
            parseInt(notes.paletteBg.slice(3,5), 16) / 255,
            parseInt(notes.paletteBg.slice(5,7), 16) / 255,
            0.85
        )

        ColumnLayout {
            anchors.centerIn: parent
            spacing: 24
            width: 320

            // Time display
            Text {
                Layout.alignment: Qt.AlignHCenter
                text: Qt.formatDateTime(new Date(), "hh:mm")
                color: notes.paletteFg
                font.pixelSize: 64
                font.bold: true
                Timer {
                    interval: 30000
                    repeat: true
                    running: true
                    onTriggered: parent.text = Qt.formatDateTime(new Date(), "hh:mm")
                }
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
                    font.pixelSize: 16
                    placeholderText: "Password"
                    // STUB: onAccepted → PAM auth via shellbridge
                }
            }
        }
    }
}
