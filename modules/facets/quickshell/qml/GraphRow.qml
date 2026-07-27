// GraphRow.qml — one row of the Session-Graph DAG tree.
//
// Renders a single flattened tree row produced by AoideSessionGraph.buildRows:
// either a project (◆), an agent session (● + agent + state + short cwd), or
// the synthetic "(unanchored)" root. Depth is expressed via left indentation.
//
// Colors come entirely from notes (no hardcoded hex). Session state drives the
// row's accent colour via stateColor(); clicking a session row emits
// `activate(windowAddress)` so the parent can route the jump through the
// shellbridge session-jump gate (the same path the Terminal-Commander chips use).

import QtQuick

Item {
    id: root

    required property var notes
    // row: { node: {…}, depth } | { synthetic: "(unanchored)", depth }
    required property var row

    // Emitted on a session-row click; carries the session's windowAddress.
    signal activate(string windowAddress)

    readonly property var node: row && row.node ? row.node : null
    readonly property int depth: row ? (row.depth || 0) : 0
    readonly property bool isProject: node && node.kind === "project"
    readonly property bool isSession: node && node.kind === "session"
    readonly property bool isSynthetic: !!(row && row.synthetic)

    // ── State → colour map (from notes; never hardcoded) ───────────────────
    //   running                       → accent
    //   awaiting / Notification-ish    → urgent
    //   done                          → fg dimmed
    //   other (idle, …)               → fg
    function stateColor(state) {
        var s = (state || "").toLowerCase()
        if (s === "running" || s === "active")
            return notes.paletteAccent
        if (s === "done")
            return notes.paletteFg
        // Blocked — a session sits on a permission prompt mid-turn, a human is
        // summoned: the Pantheon urgent role (glitchPink, base08), a hotter alarm
        // than the merely-awaiting hue below.
        if (s.indexOf("block") !== -1)
            return notes.glitchPink
        // Anything awaiting the user: Notification hook phase, "awaiting",
        // "idle" when it means "waiting for input", etc.
        if (s.indexOf("notif") !== -1 || s.indexOf("await") !== -1)
            return notes.paletteUrgent
        return notes.paletteFg
    }
    readonly property bool isDone: isSession && (node.state || "").toLowerCase() === "done"
    readonly property bool isBlocked: isSession && (node.state || "").toLowerCase().indexOf("block") !== -1

    // ── Short cwd (last two path segments) ─────────────────────────────────
    function shortCwd(cwd) {
        if (!cwd)
            return ""
        var parts = ("" + cwd).split("/").filter(function (p) { return p.length > 0 })
        if (parts.length <= 2)
            return cwd
        return "…/" + parts.slice(-2).join("/")
    }

    implicitHeight: 24

    // Blocked rows pulse — the urgent-pulse idiom from WorkspaceRow (blink_red
    // homage): a 600ms InOutQuad breath to 0.35 and back, forever, so a summoned
    // human's eye lands on the row in the DAG tree.
    SequentialAnimation on opacity {
        running: root.isBlocked
        loops: Animation.Infinite
        NumberAnimation { to: 0.35; duration: 600; easing.type: Easing.InOutQuad }
        NumberAnimation { to: 1.0;  duration: 600; easing.type: Easing.InOutQuad }
    }

    Rectangle {
        anchors.fill: parent
        radius: 4
        color: hover.hovered && root.isSession ? notes.barBg : "transparent"

        Row {
            anchors.left: parent.left
            anchors.leftMargin: 8 + root.depth * 18
            anchors.verticalCenter: parent.verticalCenter
            spacing: 6

            // ── Glyph ────────────────────────────────────────────────────
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: root.isProject ? "◆" : (root.isSession ? "●" : "")
                color: root.isProject
                       ? notes.paletteAccent
                       : (root.isSession ? root.stateColor(root.node.state) : notes.paletteFg)
                opacity: root.isDone ? 0.5 : 1.0
                font.pixelSize: 12
            }

            // ── Project name ─────────────────────────────────────────────
            Text {
                visible: root.isProject
                anchors.verticalCenter: parent.verticalCenter
                text: root.isProject ? root.node.name : ""
                color: notes.paletteFg
                font.pixelSize: 13
                font.bold: true
            }

            // ── Synthetic root label ─────────────────────────────────────
            Text {
                visible: root.isSynthetic
                anchors.verticalCenter: parent.verticalCenter
                text: root.isSynthetic ? root.row.synthetic : ""
                color: notes.paletteFg
                opacity: 0.6
                font.pixelSize: 13
                font.italic: true
            }

            // ── Session: agent ───────────────────────────────────────────
            Text {
                visible: root.isSession
                anchors.verticalCenter: parent.verticalCenter
                text: root.isSession ? root.node.agent : ""
                color: root.isSession ? root.stateColor(root.node.state) : notes.paletteFg
                opacity: root.isDone ? 0.5 : 1.0
                font.pixelSize: 13
            }

            // ── Session: state badge ─────────────────────────────────────
            Text {
                visible: root.isSession
                anchors.verticalCenter: parent.verticalCenter
                text: root.isSession ? "[" + root.node.state + "]" : ""
                color: root.isSession ? root.stateColor(root.node.state) : notes.paletteFg
                opacity: root.isDone ? 0.5 : 1.0
                font.pixelSize: 11
            }

            // ── Session: short cwd ───────────────────────────────────────
            Text {
                visible: root.isSession
                anchors.verticalCenter: parent.verticalCenter
                text: root.isSession ? root.shortCwd(root.node.cwd) : ""
                color: notes.paletteFg
                opacity: root.isDone ? 0.5 : 0.75
                font.pixelSize: 11
            }
        }

        // ── Click → session jump (sessions only) ─────────────────────────
        HoverHandler {
            id: hover
            enabled: root.isSession
        }
        MouseArea {
            anchors.fill: parent
            enabled: root.isSession
            cursorShape: root.isSession ? Qt.PointingHandCursor : Qt.ArrowCursor
            onClicked: {
                if (root.isSession)
                    root.activate(root.node.windowAddress || "")
            }
        }
    }
}
