// ClockGadget.qml — Win7-clock homage in ASCII/box-drawing chrome.
//
// A gadget for the right-edge dock (AoideAgentWidgets). Big HH:MM inside a
// double-line box-drawing frame, a date line beneath it — all monospace, all
// colors from notes (zero hardcoded hex). A single Timer ticks `now` each
// second so seconds-accurate rendering is possible; we render HH:MM (Win7
// gadget clock never showed seconds in its default face).
//
// This gadget is self-contained (no stage-file dependency): the wall clock is
// a QML Date, not agent state, so no FileView / no bridge is needed.

import QtQuick

Item {
    id: root

    // ── Note dependency (injected by the dock) ─────────────────────────────
    required property var notes

    // ── Live clock tick ────────────────────────────────────────────────────
    property var now: new Date()

    implicitHeight: column.implicitHeight

    Timer {
        interval: 1000
        repeat: true
        running: true
        onTriggered: root.now = new Date()
    }

    Column {
        id: column
        anchors.horizontalCenter: parent.horizontalCenter
        spacing: 2

        // ── Box top ──────────────────────────────────────────────────────
        // ╔══════════════╗ — width chosen to frame "HH:MM" at this size.
        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            text: "╔══════════════╗"
            color: notes.paletteAccent
            font.family: "monospace"
            font.pixelSize: 13
        }

        // ── Big time row: ║  HH:MM  ║ ────────────────────────────────────
        Row {
            anchors.horizontalCenter: parent.horizontalCenter
            spacing: 0
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: "║"
                color: notes.paletteAccent
                font.family: "monospace"
                font.pixelSize: 26
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                // Fixed-width field so the frame never jitters as digits change.
                width: 108
                horizontalAlignment: Text.AlignHCenter
                text: Qt.formatDateTime(root.now, "hh:mm")
                color: notes.paletteFg
                font.family: "monospace"
                font.pixelSize: 26
                font.bold: true
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: "║"
                color: notes.paletteAccent
                font.family: "monospace"
                font.pixelSize: 26
            }
        }

        // ── Box bottom ───────────────────────────────────────────────────
        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            text: "╚══════════════╝"
            color: notes.paletteAccent
            font.family: "monospace"
            font.pixelSize: 13
        }

        // ── Date line: e.g. "Sun 2026-07-26" ─────────────────────────────
        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            text: Qt.formatDateTime(root.now, "ddd yyyy-MM-dd")
            color: notes.paletteFg
            opacity: 0.75
            font.family: "monospace"
            font.pixelSize: 12
        }
    }
}
