// BatonGadget.qml — a mini `aoide baton` conductor, folded into the dock.
//
// A compact QML miniature of the ratatui baton TUI (pkgs/aoide/src/baton/): the
// SESSIONS mini-view under an echo of baton's tab strip, with a one-line STATUS
// footer. It reads the SAME stage documents the TUI reads —
// song/stage/sessions.json (the roster) and song/stage/graph.json (for the
// project count) — via the DrachmaState FileView idiom (atomic write-temp-rename →
// onTextChanged → recompute; FileView.text is a METHOD in quickshell 0.3.0).
//
// The state vocabulary is lifted VERBATIM from baton's theme.rs::state_glyph /
// classify so the gadget and the TUI never disagree: ♪ working, 𝄐 awaiting,
// 𝄽 idle, 𝄂 done, · unknown. Colors from notes only (zero hardcoded hex).
//
// Clicking anywhere on the body launches the full TUI in a kitty terminal via
// the global Quickshell.execDetached(list<string>) (NOT DesktopEntry's — the
// core one takes a bare argv). This is the gadget's one imperative seam; every
// other affordance is read-only rendering of the stage.

import QtQuick
import Quickshell
import Quickshell.Io

Item {
    id: root

    // ── Note dependency (injected by the dock / DesktopGadgets) ────────────
    required property var notes

    // ── Shared session state — the DAG trace link (optional) ────────────────
    // When wired (a `shared` with `tracedSessionId`, as DAG + TERMINALS get),
    // the baton row for the traced session lights its glyph + agent in the HOT
    // neon (green), so the one-neon element carries across every surface that
    // lists sessions. Dormant when null (the current instantiation sites do not
    // pass it — see the round-4 seam note); the row then renders exactly as before.
    property var shared: null
    function isTraced(sessionId) {
        return !!(shared && sessionId && sessionId.length > 0
                  && shared.tracedSessionId === sessionId)
    }

    // ── Parsed stage documents (v0 shapes; see CONTRACTS.md §4) ────────────
    property var sessionsDoc: ({ "schemaVersion": "0", "sessions": [] })
    property var graphDoc: ({ "schemaVersion": "0", "nodes": [], "edges": [] })

    property double nowMs: Date.now()

    readonly property int maxRows: 5

    // ── Stage paths (StandardPaths is unavailable → Quickshell.env HOME) ────
    readonly property string sessionsPath:
        Quickshell.env("HOME") + "/Aoide/song/stage/sessions.json"
    readonly property string graphPath:
        Quickshell.env("HOME") + "/Aoide/song/stage/graph.json"

    // ── Derived counts ─────────────────────────────────────────────────────
    readonly property var sessions:
        (sessionsDoc && sessionsDoc.sessions) ? sessionsDoc.sessions : []
    readonly property int sessionCount: sessions.length
    readonly property int projectCount: {
        var n = (graphDoc && graphDoc.nodes) ? graphDoc.nodes : []
        var c = 0
        for (var i = 0; i < n.length; i++)
            if (n[i] && n[i].kind === "project") c++
        return c
    }

    // ── State classification — mirrors baton/theme.rs::classify ────────────
    // done → 𝄂 ; await/block/notification → 𝄐 ; running/tool/active → ♪ ;
    // idle → 𝄽 ; everything else → · . Kept in lockstep with the Rust so the
    // two conductors read the same score.
    function stateGlyph(state) {
        var l = ("" + (state || "")).toLowerCase()
        if (l === "done" || l === "stop")
            return "𝄂"
        if (l.indexOf("await") !== -1 || l.indexOf("block") !== -1 || l === "notification")
            return "𝄐"
        if (l.indexOf("running") !== -1 || l.indexOf("pretooluse") !== -1
            || l.indexOf("posttooluse") !== -1 || l.indexOf("active") !== -1)
            return "♪"
        if (l.indexOf("idle") !== -1)
            return "𝄽"
        return "·"
    }
    function stateColor(state) {
        var l = ("" + (state || "")).toLowerCase()
        if (l === "done" || l === "stop")
            return notes.paletteFg
        // Blocked — a session sits on a permission prompt mid-turn and a human is
        // summoned: the Pantheon urgent role (glitchPink, base08), a hotter alarm
        // than a merely awaiting/idle session wears.
        if (l.indexOf("block") !== -1)
            return notes.glitchPink
        if (l.indexOf("await") !== -1 || l === "notification")
            return notes.paletteUrgent
        return notes.paletteAccent
    }
    function isBlocked(state) {
        return ("" + (state || "")).toLowerCase().indexOf("block") !== -1
    }
    function isDone(state) {
        var l = ("" + (state || "")).toLowerCase()
        return l === "done" || l === "stop"
    }

    // ── Elapsed: ISO-8601 startedAt → "42s"/"3m"/"2h04m"/"1d" ──────────────
    function elapsed(startedAt, nowMs) {
        if (!startedAt)
            return ""
        var t = Date.parse(startedAt)
        if (isNaN(t))
            return ""
        var sec = Math.floor((nowMs - t) / 1000)
        if (sec < 0) return ""
        if (sec < 60) return sec + "s"
        var min = Math.floor(sec / 60)
        if (min < 60) return min + "m"
        var hr = Math.floor(min / 60)
        if (hr < 24) return hr + "h" + (min % 60 < 10 ? "0" : "") + (min % 60) + "m"
        return Math.floor(hr / 24) + "d"
    }

    implicitHeight: content.implicitHeight

    Timer {
        interval: 1000
        repeat: true
        running: true
        onTriggered: root.nowMs = Date.now()
    }

    Column {
        id: content
        width: parent ? parent.width : implicitWidth
        spacing: 3

        // ══ Tab strip — echo of baton's draw_tabs (non-interactive) ════════
        // SESSIONS is the mini-view we render, so it wears the accent; the
        // others are dim, exactly as the TUI dims the inactive panels.
        Row {
            width: parent.width
            spacing: 8
            Repeater {
                model: [
                    { "n": "1", "t": "SESSIONS", "on": true },
                    { "n": "2", "t": "PROJECTS", "on": false },
                    { "n": "3", "t": "LOG", "on": false },
                    { "n": "4", "t": "STATUS", "on": false }
                ]
                delegate: Text {
                    required property var modelData
                    text: modelData.n + " " + modelData.t
                    color: notes.paletteAccent
                    opacity: modelData.on ? 1.0 : 0.45
                    font.family: "monospace"
                    font.pixelSize: 10
                    font.bold: modelData.on
                }
            }
        }

        // ══ SESSIONS mini-view ═════════════════════════════════════════════

        // Empty state — the baton's own "nothing to conduct" note.
        Text {
            visible: root.sessionCount === 0
            width: parent.width
            text: "nothing to conduct ♪(´ε｀ )"
            color: notes.paletteFg
            opacity: 0.6
            font.family: "monospace"
            font.pixelSize: 11
        }

        // Up to maxRows session rows: glyph · name · elapsed.
        Repeater {
            model: Math.min(root.sessionCount, root.maxRows)
            delegate: Row {
                id: sRow
                required property int index
                readonly property var s: root.sessions[index]
                readonly property bool traced: root.isTraced(sRow.s ? sRow.s.sessionId : "")
                width: content.width
                spacing: 6

                // Blocked rows pulse — the urgent-pulse idiom lifted verbatim from
                // WorkspaceRow (blink_red homage): a 600ms InOutQuad breath to 0.35
                // and back, forever, so a summoned human can't miss the row.
                SequentialAnimation on opacity {
                    running: root.isBlocked(sRow.s ? sRow.s.state : "")
                    loops: Animation.Infinite
                    NumberAnimation { to: 0.35; duration: 600; easing.type: Easing.InOutQuad }
                    NumberAnimation { to: 1.0;  duration: 600; easing.type: Easing.InOutQuad }
                }

                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: root.stateGlyph(sRow.s ? sRow.s.state : "")
                    // Traced session → HOT (green); else the state colour.
                    color: sRow.traced ? notes.paletteHot : root.stateColor(sRow.s ? sRow.s.state : "")
                    opacity: root.isDone(sRow.s ? sRow.s.state : "") ? 0.5 : 1.0
                    font.family: "monospace"
                    font.pixelSize: 12
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: (sRow.s && sRow.s.agent) ? sRow.s.agent : "?"
                    color: sRow.traced ? notes.paletteHot : root.stateColor(sRow.s ? sRow.s.state : "")
                    opacity: root.isDone(sRow.s ? sRow.s.state : "") ? 0.5 : 1.0
                    font.family: "monospace"
                    font.pixelSize: 11
                    font.bold: sRow.traced
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: sRow.s ? root.elapsed(sRow.s.startedAt, root.nowMs) : ""
                    color: notes.paletteFg
                    opacity: 0.6
                    font.family: "monospace"
                    font.pixelSize: 10
                }
            }
        }

        // "+N more" when the roster overflows the mini-view.
        Text {
            visible: root.sessionCount > root.maxRows
            width: parent.width
            text: "  … +" + (root.sessionCount - root.maxRows) + " more"
            color: notes.paletteFg
            opacity: 0.5
            font.family: "monospace"
            font.pixelSize: 10
        }

        // ══ STATUS footer — session + project tallies ══════════════════════
        Text {
            width: parent.width
            text: "𝄂𝄚𝅦𝄚 " + root.sessionCount + " sessions · "
                  + root.projectCount + " projects"
            color: notes.paletteAccent
            opacity: 0.7
            font.family: "monospace"
            font.pixelSize: 10
        }
    }

    // ── Body click → launch the full TUI (the one imperative seam) ─────────
    // Quickshell.execDetached takes a bare argv (list<string>) — verified in
    // the core qmltypes. Placed last so it stacks above the rows; the rows are
    // pure Text with no handlers, so nothing is stolen.
    MouseArea {
        anchors.fill: parent
        cursorShape: Qt.PointingHandCursor
        onClicked: Quickshell.execDetached(["kitty", "-e", "aoide", "baton"])
    }

    // ── sessions.json watcher (DrachmaState pattern) ─────────────────────────
    FileView {
        id: sessionsFile
        path: root.sessionsPath
        watchChanges: true
        onFileChanged: sessionsFile.reload()
        onTextChanged: {
            try {
                root.sessionsDoc = JSON.parse(sessionsFile.text())
            } catch (e) {
                console.warn("[aoide/baton] parse sessions.json:", e)
            }
        }
        Component.onCompleted: sessionsFile.reload()
    }

    // ── graph.json watcher (project count) ─────────────────────────────────
    FileView {
        id: graphFile
        path: root.graphPath
        watchChanges: true
        onFileChanged: graphFile.reload()
        onTextChanged: {
            try {
                root.graphDoc = JSON.parse(graphFile.text())
            } catch (e) {
                console.warn("[aoide/baton] parse graph.json:", e)
            }
        }
        Component.onCompleted: graphFile.reload()
    }
}
