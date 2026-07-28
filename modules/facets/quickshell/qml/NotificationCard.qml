// NotificationCard.qml — single notification card (skeleton).
//
// Renders one notification. Colors from notes; urgency mapped to
// notes.notifUrgent for urgent notifications, notes.notifFg for normal.

import QtQuick
import QtQuick.Layouts

Rectangle {
    id: root

    required property var notes
    required property var notification  // Quickshell NotificationItem

    readonly property bool isUrgent:
        notification && notification.urgency === 2  // urgency: critical

    color: notes.notifBg
    radius: 0
    border.color: isUrgent ? notes.notifUrgent : notes.windowBorder
    border.width: isUrgent ? 2 : 1
    implicitHeight: content.implicitHeight + 20

    ColumnLayout {
        id: content
        anchors {
            left: parent.left; right: parent.right
            top: parent.top
            margins: 12
        }
        spacing: 4

        Text {
            text: notification ? notification.summary : ""
            color: isUrgent ? notes.notifUrgent : notes.notifFg
            font.bold: true
            font.pixelSize: 13
            Layout.fillWidth: true
            elide: Text.ElideRight
        }

        Text {
            text: notification ? notification.body : ""
            color: notes.notifFg
            font.pixelSize: 12
            wrapMode: Text.WordWrap
            Layout.fillWidth: true
            visible: text.length > 0
        }
    }

    // Dismiss on click
    MouseArea {
        anchors.fill: parent
        onClicked: { if (notification) notification.dismiss() }
    }
}
