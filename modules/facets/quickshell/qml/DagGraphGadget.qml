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
import Quickshell.Io

Item {
    id: root

    required property var notes
    required property var bridge

    property var graph: ({ "schemaVersion": "0", "nodes": [], "edges": [] })

    readonly property string graphPath: Qt.resolvedUrl(
        (StandardPaths.writableLocation(StandardPaths.HomeLocation)) +
        "/Aoide/song/stage/graph.json"
    )

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

        // ── Tree rows ────────────────────────────────────────────────────
        Repeater {
            model: root.rows
            delegate: Item {
                id: rowItem
                width: content.width
                implicitHeight: 18
                required property int index
                required property var modelData

                readonly property var node: modelData && modelData.node ? modelData.node : null
                readonly property bool isProject: node && node.kind === "project"
                readonly property bool isSession: node && node.kind === "session"
                readonly property bool isSynthetic: !!(modelData && modelData.synthetic)
                readonly property string limb: root.limbFor(root.rows, index)

                Rectangle {
                    anchors.fill: parent
                    radius: 3
                    color: rowHover.hovered && rowItem.isSession ? notes.barBg : "transparent"
                }

                Row {
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 4

                    // ── ASCII limb ───────────────────────────────────────
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        text: rowItem.limb
                        color: notes.paletteAccent
                        opacity: 0.6
                        font.family: "monospace"
                        font.pixelSize: 12
                        visible: rowItem.limb.length > 0
                    }
                    // ── Glyph ────────────────────────────────────────────
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        text: rowItem.isProject ? "◆" : (rowItem.isSession ? "●" : "")
                        color: rowItem.isProject
                               ? notes.paletteAccent
                               : (rowItem.isSession ? root.stateColor(rowItem.node.state) : notes.paletteFg)
                        opacity: root.isDone(rowItem.node) ? 0.5 : 1.0
                        font.family: "monospace"
                        font.pixelSize: 11
                        visible: !rowItem.isSynthetic
                    }
                    // ── Project name ─────────────────────────────────────
                    Text {
                        visible: rowItem.isProject
                        anchors.verticalCenter: parent.verticalCenter
                        text: rowItem.isProject ? rowItem.node.name : ""
                        color: notes.paletteFg
                        font.family: "monospace"
                        font.pixelSize: 12
                        font.bold: true
                    }
                    // ── Synthetic label ──────────────────────────────────
                    Text {
                        visible: rowItem.isSynthetic
                        anchors.verticalCenter: parent.verticalCenter
                        text: rowItem.isSynthetic ? rowItem.modelData.synthetic : ""
                        color: notes.paletteFg
                        opacity: 0.6
                        font.family: "monospace"
                        font.italic: true
                        font.pixelSize: 12
                    }
                    // ── Session: agent ───────────────────────────────────
                    Text {
                        visible: rowItem.isSession
                        anchors.verticalCenter: parent.verticalCenter
                        text: rowItem.isSession ? (rowItem.node.agent || "") : ""
                        color: rowItem.isSession ? root.stateColor(rowItem.node.state) : notes.paletteFg
                        opacity: root.isDone(rowItem.node) ? 0.5 : 1.0
                        font.family: "monospace"
                        font.pixelSize: 12
                    }
                    // ── Session: short cwd ───────────────────────────────
                    Text {
                        visible: rowItem.isSession
                        anchors.verticalCenter: parent.verticalCenter
                        text: rowItem.isSession ? root.shortCwd(rowItem.node.cwd) : ""
                        color: notes.paletteFg
                        opacity: root.isDone(rowItem.node) ? 0.5 : 0.7
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
                root.graph = JSON.parse(graphFile.text)
            } catch (err) {
                console.warn("[aoide/dag] parse graph.json:", err)
            }
        }
        Component.onCompleted: graphFile.reload()
    }
}
