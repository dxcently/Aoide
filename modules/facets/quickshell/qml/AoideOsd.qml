// AoideOsd.qml — on-screen display (skeleton).
//
// Renders transient OSD popups: volume, brightness, microphone mute, etc.
// State fed from shellbridge stage files (stage/osd.json). Colors from notes.

import QtQuick

Item {
    id: root

    required property var notes

    // STUB: OSD popup — fades in on state change, fades out after timeout.
    Rectangle {
        id: osdPopup
        anchors.bottom: parent.bottom
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottomMargin: 48
        width: 200
        height: 48
        radius: 0
        color: notes.paletteBg
        border.color: notes.paletteAccent
        border.width: 1
        opacity: 0.0  // STUB: animate to 1 on osd state, back to 0 after timeout

        // A small centred stele: a ╱‾‾╲ pediment over a single value line
        // (icon + value). Border paletteAccent (Attic gold) kept; fade timing
        // untouched. Gold reads here only as a line/fill, never as body text.
        Column {
            anchors.centerIn: parent
            spacing: 2

            // ── Pediment: the acroterion cap ───────────────────────────────
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                text: "╱‾‾‾‾‾‾╲"
                color: notes.paletteAccent
                opacity: 0.7
                font.family: "monospace"
                font.pixelSize: 9
            }

            // ── The value line (icon + value) ──────────────────────────────
            Row {
                anchors.horizontalCenter: parent.horizontalCenter
                spacing: 12

                // Icon stub
                Rectangle {
                    width: 20; height: 20
                    radius: 0
                    color: notes.paletteAccent
                }

                // Value label stub
                Text {
                    text: "—"   // STUB: bind to stage/osd.json value
                    color: notes.paletteFg
                    font.pixelSize: 14
                    anchors.verticalCenter: parent.verticalCenter
                }
            }
        }
    }
}
