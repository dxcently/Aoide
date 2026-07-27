// AoideLauncher.qml — app/command launcher (skeleton).
//
// Quickshell native launcher — replaces rofi. Triggered by keybind (wired
// in compositor facet: SUPER+Space → shellbridge socket → show launcher).
// Colors from notes.

import QtQuick
import QtQuick.Layouts

Item {
    id: root

    required property var notes
    required property var bridge

    // ── Visibility gate ────────────────────────────────────────────────────
    // STUB: bridge/shellbridge sends { cmd: "launcher", action: "show|hide" }
    // and this widget toggles. For the skeleton, hidden by default.
    property bool visible_: false

    // ── Launcher overlay ───────────────────────────────────────────────────
    Rectangle {
        anchors.centerIn: parent
        width: 520
        height: 400
        radius: 12
        color: notes.paletteBg
        border.color: notes.paletteAccent
        border.width: 1
        visible: root.visible_

        ColumnLayout {
            anchors.fill: parent
            anchors.margins: 16
            spacing: 8

            // ── Search input ────────────────────────────────────────────
            Rectangle {
                Layout.fillWidth: true
                height: 36
                radius: 6
                color: notes.barBg
                border.color: notes.barAccent
                border.width: 1

                TextInput {
                    id: searchInput
                    anchors { fill: parent; margins: 8 }
                    color: notes.paletteFg
                    font.pixelSize: 14
                    // STUB: onTextChanged filters results list

                    // Placeholder overlay — plain TextInput has no
                    // placeholderText (that is a Controls TextField property).
                    Text {
                        anchors { left: parent.left; verticalCenter: parent.verticalCenter }
                        text: "Search apps or commands…"
                        color: notes.paletteFg
                        opacity: 0.5
                        font.pixelSize: 14
                        visible: searchInput.text.length === 0
                    }
                }
            }

            // ── Results list ────────────────────────────────────────────
            // STUB: populated from XDG desktop entries + PATH commands
            ListView {
                id: resultsList
                Layout.fillWidth: true
                Layout.fillHeight: true
                model: []  // STUB
                delegate: Rectangle {
                    width: resultsList.width
                    height: 32
                    color: ListView.isCurrentItem ? notes.paletteAccent : "transparent"
                    radius: 4
                    Text {
                        anchors { left: parent.left; leftMargin: 8; verticalCenter: parent.verticalCenter }
                        text: modelData ? modelData.name : ""
                        color: ListView.isCurrentItem ? notes.paletteBg : notes.paletteFg
                        font.pixelSize: 13
                    }
                }
            }
        }
    }
}
