// DagGraphGadget.qml — the compact, always-visible DAG view (dock gadget).
//
// The always-on sibling of AoideSessionGraph (the toggleable overlay). Both
// share the CANONICAL row-flattening in GraphModel.qml (buildRows) — feed the
// parsed graph.json in, read `rows` out — so there is one tree algorithm. The
// FileView watch on song/stage/graph.json mirrors AoideSessionGraph's exactly.
//
// Difference from the overlay: this renders ASCII tree limbs (├─ └─ with │ /
// blank continuations) in the indent instead of plain whitespace, matching the
// Unicode tree the CLI's `aoide graph view` prints (pkgs/aoide/src/graph.rs).
// The limb for each row is derived from the flat depth list by looking ahead
// for a later sibling at the same depth (machine-checked in node).
//
// Colors from notes only (zero hardcoded hex). Row click → session jump via the
// shellbridge gate (bridge.focusSession), same as the overlay.

import QtQuick
import Quickshell
import Quickshell.Io

Item {
    id: root

    required property var notes
    required property var bridge
    // Shared session state — the DAG trace link (optional). When
    // shared.tracedSessionId is set (by a TerminalManagerGadget row hover), the
    // session node whose id is `session:<that id>` is highlighted (accent border
    // + bold agent). Read-only: this gadget never writes the trace.
    property var shared: null

    property var graph: ({ "schemaVersion": "0", "nodes": [], "edges": [] })

    // ── Trace match: does this node correspond to the hovered session? ──────
    // Graph session ids are `session:<sessionId>` (graph.rs); tolerate a bare
    // id too. Empty trace → never matches.
    function isTraced(node) {
        if (!shared || !node)
            return false
        var t = shared.tracedSessionId || ""
        if (t.length === 0)
            return false
        var id = "" + (node.id || "")
        return id === "session:" + t || id === t
    }

    readonly property string graphPath:
        Quickshell.env("HOME") + "/Aoide/song/stage/graph.json"

    // ── Shared flattening (canonical in GraphModel.qml) ────────────────────
    GraphModel {
        id: graphModel
        graph: root.graph
    }
    readonly property var rows: graphModel.rows

    // ── ASCII limb prefix for row `idx` ─────────────────────────────────────
    // Builds a string of continuation columns ("│  " / "   ") for depths above
    // this row, then the branch ("├─ " / "└─ ") at this row's own depth. Depth 0
    // rows (project / synthetic roots) get no limb. "last at depth d" = no later
    // row at depth d before we pop back above d. Machine-checked in node.
    function limbFor(rows, idx) {
        var row = rows[idx]
        if (!row)
            return ""
        var depth = row.depth || 0
        if (depth === 0)
            return ""

        // For each ancestor depth a in [1..depth-1], does a later sibling exist
        // at depth a (i.e. before the list drops below a)? → "│  " else "   ".
        // For this row's own depth, is there a later sibling at `depth`? →
        // "├─ " (more coming) else "└─ " (last).
        function laterSiblingAt(startIdx, d) {
            for (var j = startIdx; j < rows.length; j++) {
                var dj = rows[j].depth || 0
                if (dj < d)
                    return false      // popped above d → no more siblings at d
                if (dj === d)
                    return true
            }
            return false
        }

        var prefix = ""
        for (var a = 1; a < depth; a++)
            prefix += laterSiblingAt(idx + 1, a) ? "│  " : "   "
        prefix += laterSiblingAt(idx + 1, depth) ? "├─ " : "└─ "
        return prefix
    }

    // ── State → colour (notes only) — mirrors GraphRow.stateColor ──────────
    function stateColor(state) {
        var s = ("" + (state || "")).toLowerCase()
        if (s === "running" || s === "active")
            return notes.paletteAccent
        if (s === "done")
            return notes.paletteFg
        if (s.indexOf("notif") !== -1 || s.indexOf("await") !== -1)
            return notes.paletteUrgent
        return notes.paletteFg
    }
    function isDone(node) {
        return node && ("" + (node.state || "")).toLowerCase() === "done"
    }
    function shortCwd(cwd) {
        if (!cwd)
            return ""
        var parts = ("" + cwd).split("/").filter(function (p) { return p.length > 0 })
        if (parts.length <= 2)
            return cwd
        return "…/" + parts.slice(-2).join("/")
    }

    // ── State → glyph — baton's theme.rs vocabulary (grammar's state tier) ──
    // ♪ working · 𝄐 awaiting · 𝄽 idle · 𝄂 done · · unknown. Kept in lockstep
    // with BatonGadget/TerminalManager so every surface reads the same score.
    function stateGlyph(state) {
        var l = ("" + (state || "")).toLowerCase()
        if (l === "done" || l === "stop")
            return "𝄂"
        if (l.indexOf("await") !== -1 || l.indexOf("block") !== -1 || l === "notification")
            return "𝄐"
        if (l.indexOf("running") !== -1 || l.indexOf("active") !== -1
            || l.indexOf("tool") !== -1)
            return "♪"
        if (l.indexOf("idle") !== -1)
            return "𝄽"
        return "·"
    }

    // ── Lowercase callout token — the reference's `optic nerve.LE.dk.002`
    // pattern, built from the REAL node id: `session:9f3a…` → `session.9f3a…`,
    // `project:aoide` → `project.aoide`. The leader line points at this.
    function calloutId(node) {
        if (!node)
            return ""
        var id = ("" + (node.id || "")).replace(":", ".").toLowerCase()
        return id
    }

    implicitHeight: content.implicitHeight

    Column {
        id: content
        width: parent ? parent.width : implicitWidth
        spacing: 1

        // ── Empty state ──────────────────────────────────────────────────
        Text {
            visible: root.rows.length === 0
            width: parent.width
            text: "no graph — aoide graph emit"
            color: notes.paletteFg
            opacity: 0.6
            font.family: "monospace"
            font.pixelSize: 12
        }

        // ── Node rows (Pantheon wireframe) ───────────────────────────────
        // Each node is a small HOLLOW OUTLINE box — double-ruled for projects,
        // single for sessions — reached by a box-drawing leader line (the tree
        // limb) and tagged with a dim lowercase callout id. Exactly ONE node
        // carries the neon accent at a time: the traced/live node (bright
        // accent border + bold label + faint fill); every other outline is
        // dimmer. The trace is driven by shared.tracedSessionId (row hover in
        // TerminalManager), unchanged.
        Repeater {
            model: root.rows
            delegate: Item {
                id: rowItem
                width: content.width
                implicitHeight: 20
                required property int index
                required property var modelData

                readonly property var node: modelData && modelData.node ? modelData.node : null
                readonly property bool isProject: node && node.kind === "project"
                readonly property bool isSession: node && node.kind === "session"
                readonly property bool isSynthetic: !!(modelData && modelData.synthetic)
                readonly property string limb: root.limbFor(root.rows, index)
                readonly property bool traced: root.isTraced(rowItem.node)
                readonly property bool done: root.isDone(rowItem.node)
                // Border/label emphasis: the traced node is the ONE neon; a
                // hovered session lifts slightly; projects read a touch stronger
                // than idle sessions; done nodes recede.
                readonly property real outlineOpacity:
                    rowItem.traced ? 1.0
                    : (rowHover.hovered && rowItem.isSession ? 0.7
                    : (rowItem.done ? 0.22
                    : (rowItem.isProject ? 0.55 : 0.4)))

                Row {
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 3

                    // ── Box-drawing leader line (tree limb) ──────────────
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        text: rowItem.limb
                        color: notes.paletteAccent
                        opacity: rowItem.traced ? 0.9 : 0.4
                        font.family: "monospace"
                        font.pixelSize: 12
                        visible: rowItem.limb.length > 0
                    }

                    // ── Synthetic label (no volume — a bare dim note) ────
                    Text {
                        visible: rowItem.isSynthetic
                        anchors.verticalCenter: parent.verticalCenter
                        text: rowItem.isSynthetic ? rowItem.modelData.synthetic : ""
                        color: notes.paletteFg
                        opacity: 0.55
                        font.family: "monospace"
                        font.italic: true
                        font.pixelSize: 12
                    }

                    // ── Hollow node box (the wireframe volume) ───────────
                    Item {
                        id: nodeBox
                        visible: !rowItem.isSynthetic && rowItem.node !== null
                        anchors.verticalCenter: parent.verticalCenter
                        implicitWidth: boxRow.implicitWidth + 12
                        implicitHeight: 16

                        // Outer outline. Traced node gets a faint accent fill.
                        Rectangle {
                            anchors.fill: parent
                            radius: 2
                            color: rowItem.traced ? notes.barBg : "transparent"
                            border.color: notes.paletteAccent
                            border.width: 1
                            opacity: rowItem.outlineOpacity
                        }
                        // Inner rule → double-line volume for PROJECT nodes.
                        Rectangle {
                            visible: rowItem.isProject
                            anchors.fill: parent
                            anchors.margins: 2
                            radius: 1
                            color: "transparent"
                            border.color: notes.paletteAccent
                            border.width: 1
                            opacity: rowItem.outlineOpacity * 0.7
                        }

                        Row {
                            id: boxRow
                            anchors.left: parent.left
                            anchors.leftMargin: 6
                            anchors.verticalCenter: parent.verticalCenter
                            spacing: 4

                            // Session STATE glyph (baton vocab). Projects skip
                            // it — the double rule already says "project".
                            Text {
                                visible: rowItem.isSession
                                anchors.verticalCenter: parent.verticalCenter
                                text: rowItem.isSession ? root.stateGlyph(rowItem.node.state) : ""
                                color: rowItem.isSession ? root.stateColor(rowItem.node.state) : notes.paletteFg
                                opacity: rowItem.done ? 0.5 : 1.0
                                font.family: "monospace"
                                font.pixelSize: 11
                            }
                            // Label: project name / session agent.
                            Text {
                                anchors.verticalCenter: parent.verticalCenter
                                text: rowItem.isProject
                                      ? (rowItem.node.name || "")
                                      : (rowItem.isSession ? (rowItem.node.agent || "?") : "")
                                color: rowItem.traced ? notes.paletteAccent
                                       : (rowItem.isSession ? root.stateColor(rowItem.node.state)
                                       : notes.paletteFg)
                                opacity: rowItem.done ? 0.5 : 1.0
                                font.family: "monospace"
                                font.pixelSize: 12
                                font.bold: rowItem.traced || rowItem.isProject
                            }
                        }
                    }

                    // ── Dim lowercase callout id (the leader's target tag) ─
                    Text {
                        visible: !rowItem.isSynthetic && rowItem.node !== null
                        anchors.verticalCenter: parent.verticalCenter
                        text: root.calloutId(rowItem.node)
                        color: notes.paletteFg
                        opacity: rowItem.traced ? 0.7 : (rowItem.done ? 0.3 : 0.45)
                        font.family: "monospace"
                        font.pixelSize: 10
                        elide: Text.ElideRight
                    }
                }

                HoverHandler { id: rowHover; enabled: rowItem.isSession }
                MouseArea {
                    anchors.fill: parent
                    enabled: rowItem.isSession
                    cursorShape: rowItem.isSession ? Qt.PointingHandCursor : Qt.ArrowCursor
                    onClicked: {
                        if (rowItem.isSession)
                            root.bridge.focusSession(rowItem.node.windowAddress || "")
                    }
                }
            }
        }
    }

    // ── File watcher — atomic hot-reload (mirrors AoideSessionGraph) ───────
    FileView {
        id: graphFile
        path: root.graphPath
        onTextChanged: {
            try {
                root.graph = JSON.parse(graphFile.text())
            } catch (err) {
                console.warn("[aoide/dag] parse graph.json:", err)
            }
        }
        Component.onCompleted: graphFile.reload()
    }
}
