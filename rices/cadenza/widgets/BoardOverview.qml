// BoardOverview.qml — the board's first tab (intent §3.3): its panes on one
// screen, AGENTS · TERMINALS · PROJECTS, and MAIL once `board.hasMailRead`
// (the mail read is not published yet; until then the pane is not drawn).
//
//   ┌─ AGENTS ─────────────────────────────────────────────────── 3/5 ┐
//   │ aoide ──────────────────────────── 3 agt · 2 working · 312k tok │
//   │ ● phase 5 slice S8 — the board read op                 [2]  #01 │
//   │   …                                   (a 9-line agent card)     │
//   │ ├─ ● subagent: read the mail store              up 12m   #01.1 │
//   │ unanchored ────────────────────────────────────────────────────  │
//   ┌─ TERMINALS ──────────────────────────────────────────────── 4 ┐
//   │ ● phase 5 slice S8 — the board read op   working  [2]    1h02m │
//   │   ~/Aoide                                    rook-lantern · aoide│
//
// The agent and terminal cards are BoardCards (sonata's conductor and
// terminals data, cadenza's paint); this file stacks the panes and scrolls.
//
// A HELPER (uppercase — never a slot), loaded by URL from BoardBody with
// `kit` and `board` (the body root: model, cards, actions, clock).
// Keys go to the agent cards: j/k or ↑/↓ select · ↵/f focus · p project ·
// u undying · x kill (BoardCards owns the selection and the action line).
import QtQuick

Item {
    id: root

    required property var kit
    required property var board

    component Use: Loader {
        required property var kit
        required property string helper
        property var props: ({})
        Component.onCompleted: {
            var p = { kit: Qt.binding(() => kit) }
            for (var k in props) p[k] = props[k]
            setSource(kit.helper(helper), p)
        }
    }

    readonly property var projects: board.model.projects

    // the two card lists, handed back by their Loaders once built
    property var agentCards: null
    property var ttyCards: null
    readonly property int agentCount: {
        var n = 0, gs = board.cards.groups
        for (var i = 0; i < gs.length; i++) n += gs[i].agents
        return n
    }
    readonly property int workingCount: {
        var n = 0, gs = board.cards.groups
        for (var i = 0; i < gs.length; i++) n += gs[i].working
        return n
    }

    function cancel() { return root.agentCards ? root.agentCards.cancel() : false }
    function handleKey(e) {
        if (!root.agentCards || !root.agentCards.handleKey(e)) return false
        Qt.callLater(root.revealSelected)
        return true
    }
    // keep the selected card in view when j/k walks off the screen
    function revealSelected() {
        var c = root.agentCards
        if (!c || c.selIndex < 0) return
        var lines = 0
        for (var i = 0; i < c.entries.length; i++) {
            var e = c.entries[i]
            if (e.type === "card" && e.row.s.sessionId === c.selId) break
            lines += e.lines
        }
        var p = c.mapToItem(col, 0, root.kit.lines(lines))
        if (p.y < flick.contentY) flick.contentY = p.y
        else if (p.y + root.kit.lines(4) > flick.contentY + flick.height)
            flick.contentY = Math.min(Math.max(0, col.implicitHeight - flick.height), p.y + root.kit.lines(4) - flick.height)
    }

    // ── layout: the panes stacked, the whole column scrolls ───────────────
    Flickable {
        id: flick
        anchors.fill: parent
        contentHeight: col.implicitHeight
        boundsBehavior: Flickable.StopAtBounds
        clip: true
        Column {
            id: col
            width: parent.width

            Use {
                width: col.width
                kit: root.kit; helper: "Pane"
                props: ({ title: "agents", glow: "bloom",
                          stat: Qt.binding(() => root.workingCount + "/" + root.agentCount),
                          rows: Qt.binding(() => root.agentCards ? root.agentCards.lineCount : 1),
                          content: agentsBody })
            }
            Use {
                width: col.width
                kit: root.kit; helper: "Pane"
                props: ({ title: "terminals", glow: "bloom",
                          stat: Qt.binding(() => "" + root.board.cards.ttys.length),
                          rows: Qt.binding(() => root.ttyCards ? root.ttyCards.lineCount : 1),
                          content: terminalsBody })
            }
            Use {
                width: col.width
                kit: root.kit; helper: "Pane"
                props: ({ title: "projects", glow: "bloom",
                          stat: Qt.binding(() => "" + root.projects.length),
                          rows: Qt.binding(() => Math.max(1, root.projects.length)),
                          content: projectsBody })
            }
            // MAIL: hidden until the mail read is published (board.hasMailRead)
            Use {
                width: col.width
                visible: root.board.hasMailRead
                kit: root.kit; helper: "Pane"
                props: ({ title: "mail", glow: "bloom",
                          stat: Qt.binding(() => "" + root.board.mailThreads.length),
                          rows: Qt.binding(() => Math.max(1, Math.min(8, root.board.mailThreads.length))),
                          content: mailBody })
            }
        }
    }

    // ══ AGENTS · TERMINALS — the cards (BoardCards, by URL) ═══════════════
    Component {
        id: agentsBody
        Use {
            width: parent ? parent.width : 0
            kit: root.kit; helper: "BoardCards"
            props: ({ board: root.board, what: "agents", project: "" })
            onLoaded: root.agentCards = item
            Component.onDestruction: if (root.agentCards === item) root.agentCards = null
        }
    }
    Component {
        id: terminalsBody
        Use {
            width: parent ? parent.width : 0
            kit: root.kit; helper: "BoardCards"
            props: ({ board: root.board, what: "terms", project: "" })
            onLoaded: root.ttyCards = item
            Component.onDestruction: if (root.ttyCards === item) root.ttyCards = null
        }
    }

    // ══ PROJECTS ══════════════════════════════════════════════════════════
    Component {
        id: projectsBody
        Column {
            width: parent ? parent.width : 0
            readonly property int w: root.kit.fit(width)
            Text {
                visible: root.projects.length === 0
                text: "no registered projects"
                color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText
            }
            Repeater {
                model: root.projects
                Item {
                    id: prow
                    required property var modelData
                    width: parent ? parent.width : 0; height: root.kit.cellH
                    readonly property int w: parent ? parent.w : 0
                    readonly property string jacks: modelData.jacks.length
                        ? modelData.jacks.map(function (j) { return "[" + j + "]" }).join("") : "—"
                    Row {
                        Text {
                            text: root.kit.padR(prow.modelData.name, 11) + " "
                            color: root.kit.path; font: root.kit.font; textFormat: Text.PlainText
                            style: Text.Outline; styleColor: root.kit.withA(color, 0.18)
                        }
                        Text {
                            text: root.kit.padR(prow.jacks, 12) + " "
                            color: prow.modelData.jacks.length ? root.kit.path : root.kit.dim
                            font: root.kit.font; textFormat: Text.PlainText
                        }
                        Text { text: root.kit.padL(prow.modelData.agents, 2); color: root.kit.number; font: root.kit.font; textFormat: Text.PlainText }
                        Text { text: " agt "; color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText }
                        Text { text: root.kit.padL(prow.modelData.terminals, 2); color: root.kit.number; font: root.kit.font; textFormat: Text.PlainText }
                        Text { text: " tty  "; color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText }
                        Text {
                            text: root.kit.padR(root.board.shortPath(prow.modelData.path), Math.max(0, prow.w - 12 - 13 - 13))
                            color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText
                        }
                    }
                    MouseArea {
                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        onClicked: root.board.openTab("project:" + prow.modelData.name)
                    }
                }
            }
        }
    }

    // ══ MAIL ══════════════════════════════════════════════════════════════
    Component {
        id: mailBody
        Column {
            width: parent ? parent.width : 0
            readonly property int w: root.kit.fit(width)
            Text {
                visible: root.board.mailThreads.length === 0
                text: "no active mail"
                color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText
            }
            Repeater {
                model: root.board.hasMailRead ? root.board.mailThreads.slice(0, 8) : []
                Row {
                    id: mrow
                    required property var modelData
                    readonly property int w: parent ? parent.w : 0
                    readonly property string box: ("" + modelData.mailbox).replace(/^[^\/]*\//, "")
                    readonly property string sender: ("" + modelData.from).replace(/^[^\/]*\//, "")
                    Text { text: root.kit.padR(mrow.box, 14) + " "; color: root.kit.path; font: root.kit.font; textFormat: Text.PlainText }
                    Text {
                        text: root.kit.padR(mrow.modelData.subject, Math.max(0, mrow.w - 15 - 12 - 5))
                        color: root.kit.ink; font: root.kit.font; textFormat: Text.PlainText
                    }
                    Text {
                        text: " " + root.kit.padR(mrow.sender, 10)
                        color: mrow.sender === "human" ? root.kit.human : root.kit.mid
                        font: root.kit.font; textFormat: Text.PlainText
                    }
                    Text { text: root.kit.padL(root.board.age(mrow.modelData.at), 5); color: root.kit.number; font: root.kit.font; textFormat: Text.PlainText }
                }
            }
        }
    }
}
