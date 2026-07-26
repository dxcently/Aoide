// AoideOsd.qml — on-screen display (skeleton).
//
// Renders transient OSD popups: volume, brightness, microphone mute, etc.
// State fed from shellbridge stage files (stage/osd.json). Colors from tokens.

import QtQuick

Item {
    id: root

    required property var tokens

    // STUB: OSD popup — fades in on state change, fades out after timeout.
    Rectangle {
        id: osdPopup
        anchors.bottom: parent.bottom
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottomMargin: 48
        width: 200
        height: 48
        radius: 24
        color: tokens.paletteBg
        border.color: tokens.paletteAccent
        border.width: 1
        opacity: 0.0  // STUB: animate to 1 on osd state, back to 0 after timeout

        Row {
            anchors.centerIn: parent
            spacing: 12

            // Icon stub
            Rectangle {
                width: 20; height: 20
                radius: 10
                color: tokens.paletteAccent
            }

            // Value label stub
            Text {
                text: "—"   // STUB: bind to stage/osd.json value
                color: tokens.paletteFg
                font.pixelSize: 14
                anchors.verticalCenter: parent.verticalCenter
            }
        }
    }
}
