// AoideSessionGraph.qml — the Session-Graph (DAG) surface (surface #9).
//
// A toggleable overlay that renders the project/session DAG staged by
// `aoide graph emit` at song/stage/graph.json. Projects are ◆ roots, agent
// sessions are ● leaves; spawned sessions nest under their parent, and
// sessions with no incoming edge group under a synthetic "(unanchored)" root.
//
// Reading discipline (CONTRACTS.md §1 / entities/Quickshell):
//   - Hot-reloads from song/stage/graph.json via a FileView, exactly like
//     NoteState watches notes.json (atomic write-temp-then-rename → onTextChanged
//     fires → bindings recompute in one pass). Tolerant of the file being
//     absent: empty state prompts `aoide graph emit`.
//   - Colors come ENTIRELY from notes (no hardcoded hex). State → color map:
//       running                         → paletteAccent
//       awaiting / Notification-ish     → paletteUrgent
//       done                            → paletteFg dimmed (0.5 opacity)
//       other (idle, …)                 → paletteFg
//
// Communication discipline:
//   - Row click → jump to that session's terminal via the shellbridge gate
//     (bridge.focusSession(windowAddress)), the same session-jump path the
//     Terminal-Commander chips use. shellbridge dispatches `aoide graph focus`
//     / `hyprctl dispatch focuswindow` — QML never shells out directly.
//   - Visibility is a bridge-driven toggle (`visible_`), mirroring AoideLauncher:
//     a Hyprland keybind execs `aoide shell graph toggle` → shellbridge →
//     flips this flag. Hidden by default.

import QtQuick
import QtQuick.Layouts
import Quickshell.Io

Item {
    id: root

    // ── Note + bridge dependencies (injected by shell.qml) ─────────────────
    required property var notes
    required property var bridge

    // ── Visibility gate ────────────────────────────────────────────────────
    // Mirrors AoideLauncher: bridge/shellbridge sends { cmd: "graph",
    // action: "show|hide|toggle" } and flips this flag. Hidden by default.
    property bool visible_: false

    // ── Parsed graph document (v0: { schemaVersion, nodes, edges }) ────────
    // Empty default → the overlay shows the "no graph yet" prompt until the
    // stage file appears.
    property var graph: ({ "schemaVersion": "0", "nodes": [], "edges": [] })

    // ── Graph path ─────────────────────────────────────────────────────────
    // Stage path: ~/Aoide/song/stage/graph.json (gitignored runtime; the nix
    // build never depends on it — checks.no-song-read enforces that).
    readonly property string graphPath: Qt.resolvedUrl(
        (StandardPaths.writableLocation(StandardPaths.HomeLocation)) +
        "/Aoide/song/stage/graph.json"
    )

    // ── Row model: flattened, indented tree ────────────────────────────────
    // Computed from graph.edges in pure JS (see buildRows). Each row is either
    // a node row { node, depth } or a synthetic root { synthetic, depth }.
    readonly property var rows: buildRows(root.graph)

    // ── Tree builder (cycle-safe) ──────────────────────────────────────────
    // Machine-checked against the v0 schema: normal nesting, unanchored
    // grouping, cycle guard (visited set), empty/absent, dangling edges.
    function buildRows(g) {
        var out = []
        if (!g || !g.nodes)
            return out

        var nodes = g.nodes || []
        var edges = g.edges || []

        // Index nodes by id.
        var byId = ({})
        for (var i = 0; i < nodes.length; i++)
            byId[nodes[i].id] = nodes[i]

        // children[fromId] = [toId, …]; hasIncoming[toId] = true.
        // Dangling edges (either endpoint missing) are ignored so a
        // hand-edited file can't inject phantom rows.
        var children = ({})
        var hasIncoming = ({})
        for (var e = 0; e < edges.length; e++) {
            var edge = edges[e]
            if (!edge || !edge.from || !edge.to)
                continue
            if (!byId[edge.from] || !byId[edge.to])
                continue
            if (!children[edge.from])
                children[edge.from] = []
            children[edge.from].push(edge.to)
            hasIncoming[edge.to] = true
        }

        // Cycle-safety: graph.json is produced cycle-free (the CLI rejects
        // cycle links), but a hand-edited file must not hang QML — guard the
        // traversal with a visited set regardless.
        var visited = ({})
        function walk(id, depth) {
            if (visited[id])
                return
            visited[id] = true
            var node = byId[id]
            if (!node)
                return
            out.push({ "node": node, "depth": depth })
            var kids = children[id] || []
            for (var k = 0; k < kids.length; k++)
                walk(kids[k], depth + 1)
        }

        // Roots: every project node, in node order.
        for (var p = 0; p < nodes.length; p++)
            if (nodes[p].kind === "project")
                walk(nodes[p].id, 0)

        // Unanchored sessions: session nodes with NO incoming edge.
        var unanchored = []
        for (var s = 0; s < nodes.length; s++) {
            var n = nodes[s]
            if (n.kind === "session" && !hasIncoming[n.id] && !visited[n.id])
                unanchored.push(n.id)
        }
        if (unanchored.length > 0) {
            out.push({ "synthetic": "(unanchored)", "depth": 0 })
            for (var u = 0; u < unanchored.length; u++)
                walk(unanchored[u], 1)
        }

        // Safety net: any session still unvisited (e.g. only reachable through
        // a cycle we broke) is surfaced at depth 0 so nothing silently vanishes.
        for (var o = 0; o < nodes.length; o++)
            if (nodes[o].kind === "session" && !visited[nodes[o].id])
                walk(nodes[o].id, 0)

        return out
    }

    // ── Graph overlay ──────────────────────────────────────────────────────
    Rectangle {
        anchors.centerIn: parent
        width: 560
        height: 460
        radius: 12
        color: notes.paletteBg
        border.color: notes.paletteAccent
        border.width: 1
        visible: root.visible_

        ColumnLayout {
            anchors.fill: parent
            anchors.margins: 16
            spacing: 8

            // ── Title ────────────────────────────────────────────────────
            Text {
                text: "Session Graph"
                color: notes.paletteAccent
                font.pixelSize: 15
                font.bold: true
                Layout.fillWidth: true
            }

            // ── Empty state ──────────────────────────────────────────────
            Text {
                text: "no graph yet — run aoide graph emit"
                color: notes.paletteFg
                opacity: 0.6
                font.pixelSize: 13
                Layout.fillWidth: true
                horizontalAlignment: Text.AlignHCenter
                visible: root.rows.length === 0
            }

            // ── DAG tree ─────────────────────────────────────────────────
            ListView {
                id: treeList
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                spacing: 2
                visible: root.rows.length > 0
                model: root.rows
                delegate: GraphRow {
                    width: treeList.width
                    notes: root.notes
                    row: modelData
                    // Click a session row → jump to its terminal via the
                    // shellbridge session-jump gate (same path as the bar chips).
                    onActivate: function (windowAddress) {
                        if (windowAddress)
                            root.bridge.focusSession(windowAddress)
                    }
                }
            }
        }
    }

    // ── File watcher — atomic hot-reload (mirrors NoteState) ───────────────
    // Tolerant of graph.json being absent: reload() on a missing path leaves
    // `graph` at its empty default, so the overlay shows the empty prompt.
    FileView {
        id: graphFile
        path: root.graphPath
        onTextChanged: {
            try {
                root.graph = JSON.parse(graphFile.text)
            } catch (err) {
                console.warn("[aoide/graph] Failed to parse graph.json:", err)
            }
        }
        Component.onCompleted: graphFile.reload()
    }
}
