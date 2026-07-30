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
        // paletteBg is a color (not a hex string) → use its .r/.g/.b channels.
        color: Qt.rgba(notes.paletteBg.r, notes.paletteBg.g, notes.paletteBg.b, 0.90)

        // A temple façade (greek-grammar §4 "Flat skeletons"): a wide ╱‾‾‾‾‾╲
        // pediment over a ‖ ‖ colonnade framing the login field. Pure chrome
        // dress — the avatar, username, password logic, and greetd behaviour are
        // untouched. Structural chrome wears bronze-verdigris (wireCyan) at ≤ 0.5
        // (§5 cap); the field keeps its Attic-gold (paletteAccent) active border.
        // The avatar is a square stele frame (radius 0), off-grammar rounding fixed.
        ColumnLayout {
            anchors.centerIn: parent
            spacing: 20
            width: 320

            // ── Pediment: the façade acroterion cap ────────────────────────
            Text {
                Layout.alignment: Qt.AlignHCenter
                text: "╱‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾‾╲"
                color: notes.wireCyan
                opacity: 0.5
                font.family: "monospace"
                font.pixelSize: 16
            }

            // User avatar placeholder — a square stele frame (radius 0).
            Rectangle {
                Layout.alignment: Qt.AlignHCenter
                width: 80; height: 80
                radius: 0
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

            // ── Password input, framed by a ‖ ‖ colonnade ──────────────────
            RowLayout {
                Layout.fillWidth: true
                spacing: 10

                Text {
                    text: "‖"
                    color: notes.wireCyan
                    opacity: 0.5
                    font.family: "monospace"
                    font.pixelSize: 30
                }

                Rectangle {
                    Layout.fillWidth: true
                    height: 44
                    radius: 0
                    color: notes.barBg
                    border.color: notes.paletteAccent
                    border.width: 1

                    TextInput {
                        id: greetPwInput
                        anchors { fill: parent; margins: 10 }
                        echoMode: TextInput.Password
                        color: notes.paletteFg
                        font.pixelSize: 15
                        // STUB: onAccepted → greetd create_session + start_session

                        // Placeholder overlay — plain TextInput has no
                        // placeholderText (Controls TextField property).
                        Text {
                            anchors { left: parent.left; verticalCenter: parent.verticalCenter }
                            text: "Password"
                            color: notes.paletteFg
                            opacity: 0.5
                            font.pixelSize: 15
                            visible: greetPwInput.text.length === 0
                        }
                    }
                }

                Text {
                    text: "‖"
                    color: notes.wireCyan
                    opacity: 0.5
                    font.family: "monospace"
                    font.pixelSize: 30
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
