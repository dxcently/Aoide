// BoardProject.qml — one registered project's tab on the board (intent §3.3):
// the project's feed on the left, its agents and terminals in a narrow rail
// on the right, the composer along the bottom.
//
//   │ 14:02 rook-lanter ● turn settled              │ ● rook-lan │
//   │ 14:02 minerva-owl ◐ Bash: cargo test          │ ◐ minerva- │
//   │ 14:03 nimble-come ↳ settled end_turn · 12 ca… │ ─ tty ──── │
//   │ 14:05 human       » re: phase 5 slice S8 — …  │ ○ brisk [2]│
//   ├───────────────────────────────────────────────┴────────────┤
//   │ to: aoide ▾ │ post: bridge not wired                        │
//
// A HELPER (uppercase — never a slot), loaded by URL from BoardBody with
// `kit`, `board` and `project`.
//
// The feed is `board.boards[project].items` — the §D answer the unbuilt
// shellbridge read op will deliver (S8–S10). Until `board.boardWired` the
// feed says `no feed — bridge not wired`. Every item's text is untrusted
// model/sender output: PlainText, one line, elided, never actionable, and
// never copied into the composer (house rule 4). The composer is drawn
// DISABLED — `boardpost` does not exist (S11/S12); its target cycles over the
// project and its agents so the shape is visible, and nothing is sent.
import QtQuick

Item {
    id: root

    required property var kit
    required property var board
    property string project: ""

    readonly property int railCells: 14
    readonly property int railW: kit.cells(railCells + 1)

    // this project's agents and terminals (the model's effective project)
    readonly property var agents: board.model.agents.filter(function (r) { return r.project === root.project })
    readonly property var terminals: board.model.terminals.filter(function (r) { return r.project === root.project })

    readonly property var answer: (board.boards && board.boards[project]) ? board.boards[project] : null
    readonly property var items: answer && answer.items ? answer.items : []

    // ── composer target: the project, or one of its agents ────────────────
    property int targetIndex: 0
    readonly property var targets: [{ label: root.project, id: "" }].concat(
        root.agents.filter(function (r) { return r.depth === 0 })
                   .map(function (r) { return { label: r.s.petname || r.s.agent || "agent", id: r.s.sessionId } }))
    readonly property var target: targets[Math.min(targetIndex, targets.length - 1)]

    function handleKey(e) {
        if (e.key === Qt.Key_J || e.key === Qt.Key_Down) { feed.flick(0, -800); return true }
        if (e.key === Qt.Key_K || e.key === Qt.Key_Up) { feed.flick(0, 800); return true }
        if (e.key === Qt.Key_End || e.key === Qt.Key_G) { feed.positionViewAtEnd(); return true }
        return false
    }

    // name for an item's session, from the live roster when it is still there
    function nameOf(it) {
        if (it.name) return "" + it.name
        // mail is signed: the sender, never the session it was delivered to
        if (it.source === "mail" && it.from) return ("" + it.from).replace(/^[^\/]*\//, "")
        var s = it.session ? board.model.byId[it.session] : null
        if (s) return s.petname || s.agent || ""
        return it.from ? ("" + it.from).replace(/^[^\/]*\//, "") : ""
    }
    function glyphOf(it) {
        if (it.source === "mail") return it.kind === "receipt" ? "→" : "»"
        if (it.source === "herald") return "!"
        switch (it.kind) {
        case "turn": return "●"
        case "awaiting": return "◐"
        case "pingback": return "↳"
        case "joined": return "+"
        case "left": return "-"
        }
        return "·"
    }
    function glyphColor(it) {
        if (it.source === "herald" || it.kind === "awaiting") return kit.urgent
        if (it.source === "mail") return kit.human
        if (it.kind === "turn") return kit.mid
        if (it.kind === "pingback") return kit.path
        return kit.dim
    }
    function textOf(it) {
        if (it.source === "mail" && it.kind === "letter")
            return (it.subject ? it.subject + " — " : "") + (it.text || "")
        if (it.source === "mail" && it.kind === "receipt") {
            var s = it.session ? board.model.byId[it.session] : null
            return (s ? (s.petname || s.agent) + ": " : "") + (it.text || "")
        }
        if (it.source === "herald") return (it.summary || "") + (it.body ? " · " + it.body : "")
        return it.text || ""
    }
    function textColor(it) {
        if (it.source === "mail") return kit.human       // a person's words
        if (it.source === "herald") return kit.urgent
        if (it.kind === "joined" || it.kind === "left") return kit.dim
        return kit.ink
    }
    function fromHuman(it) { return it.source === "mail" && /(^|\/)human$/.test("" + (it.from || "")) }

    // ══ FEED ══════════════════════════════════════════════════════════════
    Item {
        id: feedArea
        width: parent.width - root.railW
        height: parent.height - root.kit.lines(2)

        Text {
            visible: !root.board.boardWired
            text: "no feed — bridge not wired"
            color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText
        }
        Text {
            visible: root.board.boardWired && root.items.length === 0
            text: "no board items for " + root.project
            color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText
        }
        ListView {
            id: feed
            // a whole number of rows tall: pinned to the end, the top row is
            // never cut in half under the tab rule
            width: parent.width
            height: Math.floor(parent.height / root.kit.cellH) * root.kit.cellH
            visible: root.board.boardWired
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            model: root.board.boardWired ? root.items : []
            onCountChanged: Qt.callLater(feed.positionViewAtEnd)
            Component.onCompleted: Qt.callLater(feed.positionViewAtEnd)
            delegate: Row {
                id: frow
                required property var modelData
                readonly property var it: modelData
                readonly property int w: root.kit.fit(feed.width)
                readonly property string who: root.nameOf(it)
                height: root.kit.cellH
                Text {
                    text: root.board.hhmm(frow.it.at) + " "
                    color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText
                }
                Text {
                    text: root.kit.padR(frow.who, 11) + " "
                    color: root.fromHuman(frow.it) || (frow.it.source === "mail" && frow.it.kind === "receipt")
                           ? root.kit.human : root.kit.ink
                    font: root.kit.font; textFormat: Text.PlainText
                    style: Text.Outline; styleColor: root.kit.withA(color, 0.18)
                }
                Text {
                    text: root.glyphOf(frow.it) + " "
                    color: root.glyphColor(frow.it); font: root.kit.font; textFormat: Text.PlainText
                }
                Text {
                    // untrusted: one line, control characters flattened, clipped
                    text: root.kit.padR(("" + root.textOf(frow.it)).replace(/[\u0000-\u001f\u007f]+/g, " "),
                                        Math.max(0, frow.w - 6 - 12 - 2))
                    color: root.textColor(frow.it); font: root.kit.font; textFormat: Text.PlainText
                    style: Text.Outline; styleColor: root.kit.withA(color, 0.18)
                }
            }
        }
    }

    // ══ RAIL ══════════════════════════════════════════════════════════════
    Rectangle {                                   // the rail's rule
        x: feedArea.width + Math.round(root.kit.cellW / 2)
        width: 1; height: feedArea.height + Math.round(root.kit.cellH / 2)
        color: root.kit.dim
    }
    Column {
        id: rail
        x: feedArea.width + root.kit.cells(1)
        width: root.kit.cells(root.railCells)
        height: feedArea.height
        clip: true
        Text {
            visible: root.agents.length === 0
            text: "no agents"
            color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText
        }
        Repeater {
            model: root.agents
            Item {
                id: ra
                required property var modelData
                readonly property var s: modelData.s
                width: rail.width; height: root.kit.cellH
                Rectangle { anchors.fill: parent; visible: root.target && root.target.id === ra.s.sessionId; color: root.kit.select }
                Row {
                    Text {
                        text: (ra.modelData.depth ? "└" : "") + root.kit.lampGlyph(ra.s.state) + " "
                        color: ra.s.sessionId === root.board.hotId ? root.kit.hot : root.kit.lampColor(ra.s.state)
                        font: root.kit.font; textFormat: Text.PlainText
                    }
                    Text {
                        text: root.kit.padR(ra.s.petname || ra.s.agent || "agent", root.railCells - 2 - (ra.modelData.depth ? 1 : 0))
                        color: root.kit.ink; font: root.kit.font; textFormat: Text.PlainText
                    }
                }
                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: root.board.focus(ra.modelData)
                }
            }
        }
        Text {
            text: "─ tty " + "─".repeat(Math.max(0, root.railCells - 6))
            color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText
        }
        Text {
            visible: root.terminals.length === 0
            text: "none"
            color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText
        }
        Repeater {
            model: root.terminals
            Item {
                id: rt
                required property var modelData
                readonly property var s: modelData.s
                readonly property string ws: (s.workspace !== null && s.workspace !== undefined) ? " [" + s.workspace + "]" : ""
                width: rail.width; height: root.kit.cellH
                Row {
                    Text {
                        text: root.kit.lampGlyph(rt.s.state) + " "
                        color: root.kit.lampColor(rt.s.state); font: root.kit.font; textFormat: Text.PlainText
                    }
                    Text {
                        text: root.kit.padR(rt.s.petname || "shell", root.railCells - 2 - rt.ws.length)
                        color: root.kit.ink; font: root.kit.font; textFormat: Text.PlainText
                    }
                    Text { text: rt.ws; color: root.kit.path; font: root.kit.font; textFormat: Text.PlainText }
                }
                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: root.board.focus(rt.modelData)
                }
            }
        }
    }

    // ══ COMPOSER (disabled until boardpost lands) ═════════════════════════
    Rectangle {
        y: feedArea.height + Math.round(root.kit.cellH / 2)
        width: parent.width; height: 1
        color: root.kit.dim
    }
    Row {
        y: feedArea.height + root.kit.cellH
        Text { text: "to: "; color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText }
        Text {
            text: (root.target ? root.target.label : root.project) + " ▾"
            color: root.target && root.target.id ? root.kit.ink : root.kit.path
            font: root.kit.font; textFormat: Text.PlainText
            MouseArea {
                anchors.fill: parent
                cursorShape: Qt.PointingHandCursor
                onClicked: root.targetIndex = (root.targetIndex + 1) % Math.max(1, root.targets.length)
            }
        }
        Text { text: " │ "; color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText }
        Text {
            // no input exists until boardpost does: nothing here can be typed into
            text: "post: bridge not wired"
            color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText
        }
    }
}
