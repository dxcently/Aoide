// BoardOverview.qml — the board's first tab (intent §3.3): four panes on one
// screen, AGENTS · PROJECTS · TERMINALS · MAIL.
//
//   ┌─ AGENTS ───────────────────────────────────── 3/5 ┐
//   │ ● rook-lantern   aoide     working    4m  phase 5… │
//   │   [↵] focus  [p] project  [u] undying  [x] kill     │   (the selected row)
//   │ └ ● shiny-kite   aoide     working    1m  subage… │
//   └───────────────────────────────────────────────────┘
//
// A HELPER (uppercase — never a slot), loaded by URL from BoardBody with
// `kit` and `board` (the body root: model, actions, clock).
//
// Actions on the selected agent go through the bridge only, in exactly the
// shapes sonata's SessionMenu sends: focusSession; sessionAction "project"
// {project}, "undying" {state:on|off}, "kill" {} (kill confirms y/N inline,
// and is refused for subagents/app rows the way SessionMenu refuses them).
// Keys: j/k or ↑/↓ select · ↵/f focus · p project · u undying · x kill.
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

    readonly property var agents: board.model.agents
    readonly property var terminals: board.model.terminals
    readonly property var projects: board.model.projects

    // ── selection + the inline action line ────────────────────────────────
    property string selId: ""
    property string mode: ""            // "" | "project" | "kill"
    property string msg: ""
    property bool msgFailed: false
    property bool busy: false
    readonly property int selIndex: {
        for (var i = 0; i < agents.length; i++) if (agents[i].s.sessionId === selId) return i
        return -1
    }
    readonly property var selRow: selIndex >= 0 ? agents[selIndex] : null
    function select(i) {
        if (agents.length === 0) return
        i = Math.max(0, Math.min(agents.length - 1, i))
        if (agents[i].s.sessionId !== selId) { selId = agents[i].s.sessionId; mode = ""; msg = "" }
    }
    function killable(s) {
        return !!s && s.kind !== "app" && board.kindOf(s) !== "subagent"
            && ("" + (s.sessionId || "")).indexOf("sub:") !== 0 && board.isLive(s)
    }
    function isUndying(s) { return !!s && board.undyingIds.indexOf(s.sessionId) >= 0 }
    function run(action, fields) {
        var s = selRow ? selRow.s : null
        if (!s || busy) return
        if (!board.bridge || !board.bridge.sessionAction) { msgFailed = true; msg = "session actions unavailable"; return }
        busy = true; msgFailed = false; msg = "waiting for aoide…"; mode = ""
        board.bridge.sessionAction(s.sessionId, action, fields, function (r) {
            root.busy = false
            root.msgFailed = !(r && r.ok)
            root.msg = (r && r.message) ? ("" + r.message) : (root.msgFailed ? "action failed" : "done")
        })
    }
    function cancel() {
        if (mode !== "") { mode = ""; return true }
        return false
    }
    function handleKey(e) {
        var s = selRow ? selRow.s : null
        if (mode === "kill") {
            if (e.key === Qt.Key_Y) { run("kill", {}); return true }
            mode = ""; msg = "kill cancelled"; msgFailed = false; return true
        }
        if (mode === "project") {
            var n = e.key - Qt.Key_0
            if (n === 0) { run("project", { project: "" }); return true }
            if (n >= 1 && n <= 9 && n <= projects.length) { run("project", { project: projects[n - 1].name }); return true }
            mode = ""; return true
        }
        switch (e.key) {
        case Qt.Key_J: case Qt.Key_Down: select(selIndex + 1); return true
        case Qt.Key_K: case Qt.Key_Up:   select(selIndex < 0 ? 0 : selIndex - 1); return true
        case Qt.Key_Return: case Qt.Key_Enter: case Qt.Key_F:
            if (selRow) board.focus(selRow); return true
        case Qt.Key_P: if (s) { mode = "project"; msg = "" } return true
        case Qt.Key_U: if (s) run("undying", { state: isUndying(s) ? "off" : "on" }); return true
        case Qt.Key_X: if (killable(s)) { mode = "kill"; msg = "" } return true
        }
        return false
    }

    // ── layout: the four panes stacked, the whole column scrolls ──────────
    Flickable {
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
                          stat: Qt.binding(() => root.board.model.working + "/" + root.agents.length),
                          rows: Qt.binding(() => Math.max(1, root.agents.length + (root.selRow ? 1 : 0))),
                          content: agentsBody })
            }
            Use {
                width: col.width
                kit: root.kit; helper: "Pane"
                props: ({ title: "projects", glow: "bloom",
                          stat: Qt.binding(() => "" + root.projects.length),
                          rows: Qt.binding(() => Math.max(1, root.projects.length)),
                          content: projectsBody })
            }
            Use {
                width: col.width
                kit: root.kit; helper: "Pane"
                props: ({ title: "terminals", glow: "bloom",
                          stat: Qt.binding(() => "" + root.terminals.length),
                          rows: Qt.binding(() => Math.max(1, root.terminals.length)),
                          content: terminalsBody })
            }
            Use {
                width: col.width
                kit: root.kit; helper: "Pane"
                props: ({ title: "mail", glow: "bloom",
                          stat: Qt.binding(() => root.board.boardWired ? "" + root.board.mailThreads.length : ""),
                          rows: Qt.binding(() => Math.max(1, Math.min(8, root.board.mailThreads.length))),
                          content: mailBody })
            }
        }
    }

    // ══ AGENTS ════════════════════════════════════════════════════════════
    Component {
        id: agentsBody
        Column {
            width: parent ? parent.width : 0
            readonly property int w: root.kit.fit(width)
            Text {
                visible: root.agents.length === 0
                text: "no live agents"
                color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText
            }
            Repeater {
                model: root.agents
                Column {
                    id: arow
                    required property var modelData
                    required property int index
                    readonly property var s: modelData.s
                    readonly property bool sel: s.sessionId === root.selId
                    readonly property bool hot: s.sessionId === root.board.hotId
                    readonly property int w: parent ? parent.w : 0
                    // name 14 · project 9 · state 9 · age 4 · title (rest)
                    readonly property int titleCells: Math.max(0, w - 2 - 15 - 10 - 10 - 5 - 1)
                    width: parent ? parent.width : 0

                    Item {
                        width: arow.width; height: root.kit.cellH
                        Rectangle { anchors.fill: parent; visible: arow.sel; color: root.kit.select }
                        Row {
                            Text {
                                visible: arow.modelData.depth > 0
                                text: "└ "
                                color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText
                            }
                            Text {
                                text: root.kit.lampGlyph(arow.s.state) + " "
                                color: arow.hot ? root.kit.hot : root.kit.lampColor(arow.s.state)
                                font: root.kit.font; textFormat: Text.PlainText
                                style: Text.Outline; styleColor: root.kit.withA(color, 0.18)
                            }
                            Text {
                                text: root.kit.padR(arow.s.petname || arow.s.agent || "agent", arow.modelData.depth ? 12 : 14) + " "
                                color: arow.sel ? root.kit.match : root.kit.ink
                                font: root.kit.font; textFormat: Text.PlainText
                                style: Text.Outline; styleColor: root.kit.withA(color, 0.18)
                            }
                            Text {
                                text: root.kit.padR(arow.modelData.project || "—", 9) + " "
                                color: arow.modelData.project ? root.kit.path : root.kit.dim
                                font: root.kit.font; textFormat: Text.PlainText
                            }
                            Text {
                                text: root.kit.padR(arow.s.state || "", 9) + " "
                                color: arow.s.state === "awaiting" ? root.kit.urgent : (arow.sel ? root.kit.match : root.kit.mid)
                                font: root.kit.font; textFormat: Text.PlainText
                            }
                            Text {
                                text: root.kit.padL(root.board.age(arow.modelData.since), 4) + " "
                                color: root.kit.number; font: root.kit.font; textFormat: Text.PlainText
                            }
                            Text {
                                text: " " + root.kit.padR(arow.s.title || arow.s.agent || "", arow.titleCells)
                                color: arow.sel ? root.kit.match : root.kit.dim
                                font: root.kit.font; textFormat: Text.PlainText
                            }
                        }
                        MouseArea {
                            anchors.fill: parent
                            onClicked: { root.select(arow.index); root.board.forceActiveFocus() }
                            onDoubleClicked: root.board.focus(arow.modelData)
                        }
                    }

                    // the selected row's action line
                    Item {
                        visible: arow.sel
                        width: arow.width; height: visible ? root.kit.cellH : 0
                        Rectangle { anchors.fill: parent; color: root.kit.select }
                        Row {
                            x: root.kit.cells(2)
                            visible: root.mode === "" && root.msg === ""
                            Repeater {
                                model: [
                                    { k: "↵", t: "focus",   on: true,  a: "focus" },
                                    { k: "p", t: "project", on: true,  a: "project" },
                                    { k: "u", t: root.isUndying(arow.s) ? "undying ✓" : "undying", on: true, a: "undying" },
                                    { k: "x", t: root.killable(arow.s) ? "kill" : "kill n/a", on: root.killable(arow.s), a: "kill" }
                                ]
                                Row {
                                    id: act
                                    required property var modelData
                                    Text { text: "[" + act.modelData.k + "] "; color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText }
                                    Text {
                                        text: act.modelData.t + "  "
                                        color: act.modelData.on ? root.kit.match : root.kit.dim
                                        font: root.kit.font; textFormat: Text.PlainText
                                        MouseArea {
                                            anchors.fill: parent
                                            enabled: act.modelData.on
                                            onClicked: {
                                                var a = act.modelData.a
                                                if (a === "focus") root.board.focus(arow.modelData)
                                                else if (a === "project") root.mode = "project"
                                                else if (a === "undying") root.run("undying", { state: root.isUndying(arow.s) ? "off" : "on" })
                                                else if (a === "kill") root.mode = "kill"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        // project picker: [0] auto [1] aoide …
                        Row {
                            x: root.kit.cells(2)
                            visible: root.mode === "project"
                            Text { text: "project: "; color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText }
                            Repeater {
                                model: [{ n: 0, name: "" }].concat(root.projects.slice(0, 9).map(function (p, i) { return { n: i + 1, name: p.name } }))
                                Row {
                                    id: pick
                                    required property var modelData
                                    Text { text: "[" + pick.modelData.n + "] "; color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText }
                                    Text {
                                        text: (pick.modelData.name || "auto") + "  "
                                        color: pick.modelData.name ? root.kit.path : root.kit.match
                                        font: root.kit.font; textFormat: Text.PlainText
                                        MouseArea { anchors.fill: parent; onClicked: root.run("project", { project: pick.modelData.name }) }
                                    }
                                }
                            }
                        }
                        // kill confirm, y/N
                        Text {
                            x: root.kit.cells(2)
                            visible: root.mode === "kill"
                            text: "kill " + (arow.s.petname || arow.s.sessionId) + "?  [y/N]"
                            color: root.kit.urgent; font: root.kit.font; textFormat: Text.PlainText
                        }
                        // the bridge's answer
                        Text {
                            x: root.kit.cells(2)
                            width: parent.width - x
                            visible: root.mode === "" && root.msg !== ""
                            text: (root.msgFailed ? "✕ " : "→ ") + root.msg
                            elide: Text.ElideRight
                            color: root.msgFailed ? root.kit.urgent : root.kit.match
                            font: root.kit.font; textFormat: Text.PlainText
                            MouseArea { anchors.fill: parent; onClicked: root.msg = "" }
                        }
                    }
                }
            }
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

    // ══ TERMINALS ═════════════════════════════════════════════════════════
    Component {
        id: terminalsBody
        Column {
            width: parent ? parent.width : 0
            readonly property int w: root.kit.fit(width)
            Text {
                visible: root.terminals.length === 0
                text: "no conducted terminals"
                color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText
            }
            Repeater {
                model: root.terminals
                Item {
                    id: trow
                    required property var modelData
                    readonly property var s: modelData.s
                    readonly property int w: parent ? parent.w : 0
                    width: parent ? parent.width : 0; height: root.kit.cellH
                    Row {
                        Text {
                            text: root.kit.lampGlyph(trow.s.state) + " "
                            color: trow.s.sessionId === root.board.hotId ? root.kit.hot : root.kit.lampColor(trow.s.state)
                            font: root.kit.font; textFormat: Text.PlainText
                        }
                        Text {
                            text: root.kit.padR(trow.s.petname || trow.s.agent || "shell", 14) + " "
                            color: root.kit.ink; font: root.kit.font; textFormat: Text.PlainText
                            style: Text.Outline; styleColor: root.kit.withA(color, 0.18)
                        }
                        Text {
                            text: root.kit.padR(trow.s.workspace !== null && trow.s.workspace !== undefined
                                                ? "[" + trow.s.workspace + "]" : "[·]", 5) + " "
                            color: root.kit.path; font: root.kit.font; textFormat: Text.PlainText
                        }
                        Text {
                            text: root.kit.padR(trow.modelData.project || "—", 9) + " "
                            color: trow.modelData.project ? root.kit.path : root.kit.dim
                            font: root.kit.font; textFormat: Text.PlainText
                        }
                        Text {
                            text: root.kit.padR(root.board.shortPath(trow.s.cwd), Math.max(0, trow.w - 2 - 15 - 6 - 10))
                            color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText
                        }
                    }
                    MouseArea {
                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        onClicked: root.board.focus(trow.modelData)
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
                visible: !root.board.boardWired
                text: "no mail view — bridge not wired"
                color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText
            }
            Text {
                visible: root.board.boardWired && root.board.mailThreads.length === 0
                text: "no active mail"
                color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText
            }
            Repeater {
                model: root.board.boardWired ? root.board.mailThreads.slice(0, 8) : []
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
