// AoideBar.qml — workspaces bar (skeleton).
//
// Renders the top/bottom bar: workspace switcher, active window title,
// agent-session indicators, system tray, clock. All colors / font sizes /
// geometry come from the notes property (no hardcoded values).
//
// The Terminal-Commander session-jump widget lives here: clicking an agent
// session indicator calls bridge.focusSession(address) which routes through
// shellbridge → hyprctl dispatch focuswindow.

import QtQuick
import QtQuick.Layouts

Item {
    id: root

    // ── Note + bridge dependencies (injected by shell.qml) ────────────────
    required property var notes
    required property var bridge

    // ── Bar geometry ───────────────────────────────────────────────────────
    // Height driven by notes when a geometry tier is added (v1). For now,
    // a reasonable default.
    implicitHeight: 36

    // ── Background ────────────────────────────────────────────────────────
    Rectangle {
        anchors.fill: parent
        color: notes.barBg

        RowLayout {
            anchors.fill: parent
            anchors.leftMargin: 8
            anchors.rightMargin: 8
            spacing: 8

            // ── Workspace indicators ─────────────────────────────────────
            // STUB: will bind to Hyprland IPC workspace list via shellbridge
            // state files (song/stage/workspaces.json).
            WorkspaceRow {
                id: workspaces
                notes: root.notes
                Layout.fillHeight: true
            }

            // ── Active window title ────────────────────────────────────
            Text {
                id: windowTitle
                text: "Aoide"   // STUB: bind to stage/active-window.json
                color: notes.barFg
                font.pixelSize: 13
                Layout.fillWidth: true
                elide: Text.ElideRight
            }

            // ── Agent session indicators (Terminal-Commander) ──────────
            // STUB: bind to stage/sessions.json; each entry gets a click
            // handler that calls bridge.focusSession(session.address).
            Row {
                id: sessionIndicators
                spacing: 4
                Layout.fillHeight: true

                Repeater {
                    model: []  // STUB: stage/sessions.json entries
                    delegate: SessionChip {
                        notes: root.notes
                        sessionData: modelData
                        onClicked: root.bridge.focusSession(sessionData.address)
                    }
                }
            }

            // ── Clock ─────────────────────────────────────────────────
            Text {
                id: clock
                text: Qt.formatDateTime(new Date(), "hh:mm")
                color: notes.barAccent
                font.pixelSize: 13
                Timer {
                    interval: 30000
                    repeat: true
                    running: true
                    onTriggered: clock.text = Qt.formatDateTime(new Date(), "hh:mm")
                }
            }
        }
    }
}
