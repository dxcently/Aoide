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

    // ── Neon-dominance + drawn-leader tuning (round 2, tunable seam) ────────
    // The refs' core rule: one blazing element against a dim wireframe field.
    // Non-hot layers rest LOW; only the traced/live node blazes (border 2, full
    // accent, bold label, a soft two-ring halo). Leaders are DRAWN (a per-row
    // Canvas painting the refs' kinked elbow) instead of box-drawing glyphs.
    property real dimProject: 0.45   // project double-rule at rest
    property real dimIdle: 0.35      // idle session outline at rest
    property real dimDone: 0.2       // done node — nearly gone
    property real dimHover: 0.6      // a hovered (non-traced) session lifts
    property real dimCallout: 0.35   // lowercase callout label at rest
    property real haloOpacity1: 0.35 // traced glow — inner ring (grown +2)
    property real haloOpacity2: 0.15 // traced glow — mid ring (grown +4)
    property real haloOpacity3: 0.08 // traced glow — outer bloom ring (grown +6)
    property int indentStep: 16      // px per tree depth (leader gutter column)
    property int leaderKink: 4       // 45° elbow run before the horizontal
    property int leaderDot: 2        // terminal-dot radius where leader meets box
    property real leaderWidth: 1.5   // leader stroke width (px)

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

    // ── Drawn-leader spec for row `idx` (round 2) ───────────────────────────
    // The machine data behind limbFor, but for a painted leader instead of box
    // glyphs: `depth` (own tree depth), `conts` (ancestor depths that still have
    // a later sibling → a full-height continuation rule), and `throughOwn` (does
    // THIS row's own column continue below to a later sibling → ├ vs └). The
    // Canvas turns this into the refs' kinked elbow. Same laterSiblingAt rule as
    // limbFor, so the drawn tree matches the CLI's Unicode tree exactly.
    function leaderSpec(rows, idx) {
        var row = rows[idx]
        if (!row)
            return { "depth": 0, "conts": [], "throughOwn": false }
        var depth = row.depth || 0
        function laterSiblingAt(startIdx, d) {
            for (var j = startIdx; j < rows.length; j++) {
                var dj = rows[j].depth || 0
                if (dj < d)
                    return false
                if (dj === d)
                    return true
            }
            return false
        }
        var conts = []
        for (var a = 1; a < depth; a++)
            if (laterSiblingAt(idx + 1, a))
                conts.push(a)
        return { "depth": depth, "conts": conts,
                 "throughOwn": laterSiblingAt(idx + 1, depth) }
    }

    // ── State → colour (notes only) — mirrors GraphRow.stateColor ──────────
    function stateColor(state) {
        var s = ("" + (state || "")).toLowerCase()
        if (s === "running" || s === "active")
            return notes.paletteAccent
        if (s === "done")
            return notes.paletteFg
        // Blocked — a session waits on a permission prompt mid-turn: the Pantheon
        // urgent role (glitchPink, base08), hotter than a notif/await hue.
        if (s.indexOf("block") !== -1)
            return notes.glitchPink
        if (s.indexOf("notif") !== -1 || s.indexOf("await") !== -1)
            return notes.paletteUrgent
        return notes.paletteFg
    }
    function isDone(node) {
        return node && ("" + (node.state || "")).toLowerCase() === "done"
    }
    function isBlocked(node) {
        return node && ("" + (node.state || "")).toLowerCase().indexOf("block") !== -1
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
                implicitHeight: 22
                required property int index
                required property var modelData

                readonly property var node: modelData && modelData.node ? modelData.node : null
                readonly property int depth: modelData ? (modelData.depth || 0) : 0
                readonly property bool isProject: node && node.kind === "project"
                readonly property bool isSession: node && node.kind === "session"
                readonly property bool isSynthetic: !!(modelData && modelData.synthetic)
                readonly property bool traced: root.isTraced(rowItem.node)
                readonly property bool done: root.isDone(rowItem.node)
                // Neon dominance: the traced node is the ONE blaze; a hovered
                // session lifts; projects read a touch stronger than idle
                // sessions; done nodes nearly vanish. At rest the whole graph
                // sits dim — the refs' "one bright element" rule.
                readonly property real outlineOpacity:
                    rowItem.traced ? 1.0
                    : (rowHover.hovered && rowItem.isSession ? root.dimHover
                    : (rowItem.done ? root.dimDone
                    : (rowItem.isProject ? root.dimProject : root.dimIdle)))
                readonly property bool blocked: root.isBlocked(rowItem.node)

                // Blocked node pulses — the urgent-pulse idiom from WorkspaceRow
                // (blink_red homage): a 600ms InOutQuad breath to 0.35 and back,
                // forever, so the summoned human's eye finds the node in the DAG.
                SequentialAnimation on opacity {
                    running: rowItem.blocked
                    loops: Animation.Infinite
                    NumberAnimation { to: 0.35; duration: 600; easing.type: Easing.InOutQuad }
                    NumberAnimation { to: 1.0;  duration: 600; easing.type: Easing.InOutQuad }
                }

                Row {
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 3

                    // ── Drawn tree leader (round 2) — the refs' kinked elbow ─
                    // A per-row Canvas painting into the indent gutter: full-
                    // height continuation rules for ancestors with later
                    // siblings, then this row's branch — a vertical drop that
                    // taps a 45° elbow into the child box with a terminal dot.
                    // Opacity matches the child's dim level (hot child =
                    // brighter). requestPaint only on trace/size change — never
                    // per-frame.
                    Canvas {
                        id: leaderCanvas
                        visible: rowItem.depth > 0
                        anchors.verticalCenter: parent.verticalCenter
                        width: rowItem.depth * root.indentStep
                        height: rowItem.implicitHeight
                        onPaint: {
                            var ctx = getContext("2d")
                            ctx.reset()
                            ctx.clearRect(0, 0, width, height)
                            var spec = root.leaderSpec(root.rows, rowItem.index)
                            if (spec.depth <= 0)
                                return
                            var step = root.indentStep
                            var h = height, cy = h / 2
                            // Hot child → green leader + dot; rest → wireCyan.
                            var leaderColor = rowItem.traced ? notes.paletteHot : notes.wireCyan
                            ctx.strokeStyle = leaderColor
                            ctx.fillStyle = leaderColor
                            ctx.lineWidth = root.leaderWidth
                            ctx.globalAlpha = rowItem.traced ? 0.95
                                : (rowItem.done ? root.dimDone : root.dimIdle + 0.1)
                            // ancestor continuation columns
                            for (var i = 0; i < spec.conts.length; i++) {
                                var cx = (spec.conts[i] - 0.5) * step
                                ctx.beginPath(); ctx.moveTo(cx, 0); ctx.lineTo(cx, h); ctx.stroke()
                            }
                            // own branch column (through to bottom if a later sibling follows)
                            var bx = (spec.depth - 0.5) * step
                            var endY = spec.throughOwn ? h : cy
                            ctx.beginPath(); ctx.moveTo(bx, 0); ctx.lineTo(bx, endY); ctx.stroke()
                            // elbow: tap the vertical above center, 45° kink into the child box
                            var k = root.leaderKink
                            ctx.beginPath()
                            ctx.moveTo(bx, cy - k)
                            ctx.lineTo(bx + k, cy)
                            ctx.lineTo(width - root.leaderDot, cy)
                            ctx.stroke()
                            // terminal dot where the leader meets the child box
                            ctx.beginPath()
                            ctx.arc(width - root.leaderDot, cy, root.leaderDot, 0, 2 * Math.PI)
                            ctx.fill()
                        }
                        onWidthChanged: requestPaint()
                        Component.onCompleted: requestPaint()
                        Connections {
                            target: rowItem
                            function onTracedChanged() { leaderCanvas.requestPaint() }
                        }
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

                        // ── Neon halo (traced node only) ─────────────────
                        // The depth-stack trick used as a GLOW instead of an
                        // offset: THREE transparent rings, the box grown +6/+4/+2,
                        // in the HOT neon (green) so the traced node reads as a
                        // glowing bloom, not just a bordered box. Declared first →
                        // they sit behind the outline. Border-only, no MouseArea
                        // (input-inert). The rose accent stays chrome; this is the
                        // one green element under the pointer.
                        Rectangle {
                            visible: rowItem.traced
                            anchors.fill: parent
                            anchors.margins: -6
                            radius: 5
                            color: "transparent"
                            border.color: notes.paletteHot
                            border.width: 1
                            opacity: root.haloOpacity3
                        }
                        Rectangle {
                            visible: rowItem.traced
                            anchors.fill: parent
                            anchors.margins: -4
                            radius: 4
                            color: "transparent"
                            border.color: notes.paletteHot
                            border.width: 1
                            opacity: root.haloOpacity2
                        }
                        Rectangle {
                            visible: rowItem.traced
                            anchors.fill: parent
                            anchors.margins: -2
                            radius: 3
                            color: "transparent"
                            border.color: notes.paletteHot
                            border.width: 1
                            opacity: root.haloOpacity1
                        }

                        // Outer outline. Traced node blazes: 2px HOT (green)
                        // border, full bright, a faint fill. Rest nodes wear the
                        // cool wireframe field — PROJECT volumes violet (base0E),
                        // SESSION volumes wireCyan (base0C) — one green element
                        // blazing against that multicolor field.
                        Rectangle {
                            anchors.fill: parent
                            radius: 2
                            color: rowItem.traced ? notes.barBg : "transparent"
                            border.color: rowItem.traced ? notes.paletteHot
                                          : (rowItem.isProject ? notes.violet : notes.wireCyan)
                            border.width: rowItem.traced ? 2 : 1
                            opacity: rowItem.outlineOpacity
                        }
                        // Inner rule → double-line volume for PROJECT nodes (violet).
                        Rectangle {
                            visible: rowItem.isProject
                            anchors.fill: parent
                            anchors.margins: 2
                            radius: 1
                            color: "transparent"
                            border.color: notes.violet
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
                                color: rowItem.traced ? notes.paletteHot
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
                        opacity: rowItem.traced ? 0.7 : (rowItem.done ? root.dimDone : root.dimCallout)
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
        watchChanges: true
        onFileChanged: graphFile.reload()
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
