// AoideNotifications.qml — native notification daemon (skeleton).
//
// Quickshell implements org.freedesktop.Notifications natively — no mako,
// no swaync. This widget owns the notification surface entirely.
//
// A NotificationServer spike (actions + inline reply) is planned per
// entities/Quickshell. This skeleton wires the structure; the spike fills
// the protocol implementation.

import QtQuick
import QtQuick.Layouts
import Quickshell.Services.Notifications

Item {
    id: root

    required property var tokens
    required property var bridge

    // ── Notification server (owns the D-Bus name) ─────────────────────────
    // STUB: NotificationServer is Quickshell's built-in. When instantiated it
    // claims org.freedesktop.Notifications on the session bus.
    NotificationServer {
        id: notifServer
        // Spike: add actions and inline-reply support here when the QML API
        // is confirmed. For the skeleton, the server is declared but the
        // notification list is not yet rendered.
    }

    // ── Notification list overlay ─────────────────────────────────────────
    // Slides in from the right edge. Populated from notifServer.notifications.
    // Colors come entirely from tokens.
    Column {
        id: notifList
        anchors.top: parent.top
        anchors.right: parent.right
        anchors.margins: 12
        spacing: 6
        width: 360

        Repeater {
            model: notifServer.notifications
            delegate: NotificationCard {
                tokens: root.tokens
                notification: modelData
                width: notifList.width
            }
        }
    }
}
