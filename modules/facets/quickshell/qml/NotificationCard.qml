// NotificationCard.qml — single notification card (votive stele).
//
// Renders one notification, dressed in sonata's typographic-Greek grammar
// (song/songbook/sonata/design/greek-grammar.md §4 "Flat skeletons"): a
// ╱‾‾╲ pediment carrying the app name, ║ engaged-column rails down the edges,
// and a stylobate base rule. Function/data/geometry are UNCHANGED — the
// existing notifUrgent/windowBorder border logic is re-dressed as the rails.
// Urgent (urgency === 2) swaps the rail colour to notifUrgent (terracotta) and
// pulses. Colors from notes only.

import QtQuick
import QtQuick.Layouts

Rectangle {
    id: root

    required property var notes
    required property var notification  // Quickshell NotificationItem

    readonly property bool isUrgent:
        notification && notification.urgency === 2  // urgency: critical

    // The re-dressed border: the same isUrgent → notifUrgent / windowBorder
    // decision that used to stroke the card border now colours the ║ rails,
    // pediment, and stylobate. windowBorder is the bronze-verdigris structural
    // role, so at rest the chrome rests ≤ 0.5 (§5 verdigris cap); the terracotta
    // summons is allowed full when urgent.
    readonly property color railColor: isUrgent ? notes.notifUrgent : notes.windowBorder
    readonly property real chromeOpacity: isUrgent ? 0.9 : 0.5

    color: notes.notifBg
    radius: 0
    border.width: 0
    implicitHeight: content.implicitHeight + 22

    // Urgent pulse — the terracotta summons breathes (WorkspaceRow blink idiom).
    SequentialAnimation on opacity {
        running: root.isUrgent
        loops: Animation.Infinite
        NumberAnimation { to: 0.6; duration: 600; easing.type: Easing.InOutQuad }
        NumberAnimation { to: 1.0; duration: 600; easing.type: Easing.InOutQuad }
    }

    // ── ║ engaged-column rails (the re-dressed border) ─────────────────────
    Rectangle {
        anchors { left: parent.left; top: parent.top; bottom: parent.bottom }
        width: root.isUrgent ? 2 : 1
        radius: 0
        color: root.railColor
        opacity: root.chromeOpacity
    }
    Rectangle {
        anchors { right: parent.right; top: parent.top; bottom: parent.bottom }
        width: root.isUrgent ? 2 : 1
        radius: 0
        color: root.railColor
        opacity: root.chromeOpacity
    }

    ColumnLayout {
        id: content
        anchors {
            left: parent.left; right: parent.right
            top: parent.top
            margins: 12
        }
        spacing: 4

        // ── Pediment: ╱‾‾╲ acroterion carrying the app name ────────────────
        Text {
            Layout.fillWidth: true
            horizontalAlignment: Text.AlignHCenter
            text: "╱‾‾‾‾‾‾╲"
            color: root.railColor
            opacity: root.chromeOpacity
            font.family: "monospace"
            font.pixelSize: 11
        }
        Text {
            Layout.fillWidth: true
            horizontalAlignment: Text.AlignHCenter
            text: notification && notification.appName ? notification.appName : ""
            visible: text.length > 0
            color: notes.notifFg
            opacity: 0.55
            font.pixelSize: 10
            elide: Text.ElideRight
        }

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

        // ── Stylobate: the base course rule the stele stands on ────────────
        Text {
            Layout.fillWidth: true
            horizontalAlignment: Text.AlignHCenter
            text: "▔▔▔▔▔▔▔▔▔▔▔▔"
            color: root.railColor
            opacity: root.chromeOpacity
            font.family: "monospace"
            font.pixelSize: 10
            clip: true
        }
    }

    // Dismiss on click
    MouseArea {
        anchors.fill: parent
        onClicked: { if (notification) notification.dismiss() }
    }
}
