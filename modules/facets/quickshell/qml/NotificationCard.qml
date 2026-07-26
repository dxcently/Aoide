// NotificationCard.qml — single notification card (skeleton).
//
// Renders one notification. Colors from tokens; urgency mapped to
// tokens.notifUrgent for urgent notifications, tokens.notifFg for normal.

import QtQuick
import QtQuick.Layouts

Rectangle {
    id: root

    required property var tokens
    required property var notification  // Quickshell NotificationItem

    readonly property bool isUrgent:
        notification && notification.urgency === 2  // urgency: critical

    color: tokens.notifBg
    radius: 6
    border.color: isUrgent ? tokens.notifUrgent : tokens.windowBorder
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
            color: isUrgent ? tokens.notifUrgent : tokens.notifFg
            font.bold: true
            font.pixelSize: 13
            Layout.fillWidth: true
            elide: Text.ElideRight
        }

        Text {
            text: notification ? notification.body : ""
            color: tokens.notifFg
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
