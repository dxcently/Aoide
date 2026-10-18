// BoardCards.qml — the board's agent cards and terminal cards (intent §3.3):
// sonata's conductor and terminals DATA, drawn in cadenza's termui grammar.
// Loaded by URL inside a kit Pane by BoardOverview (every project, grouped)
// and BoardProject (one project, no group chrome).
//
//   aoide ─────────────────────────────────── 3 agt · 2 working · 312k tok
//   ● phase 5 slice S8 — the board read op                     [2]   #01
//     claude / claude-opus-5-5                                  working
//     rook-lantern · …0001 · yomi · aoide
//     » land the board read op behind hasBoardFeed
//     ▸ Bash: cargo test -p aoide-conduct -- graph::ties
//     The ties block needs the spawned edge before the anchors edge,
//     otherwise the clique collapses; reading graph.rs to confirm the
//     order the daemon writes them in, then…
//     ~/Aoide/…/crates/conduct             ctx ██████▎░ 120k/200k  up 1h02m
//   ├─ ● subagent: read the mail store                   up 12m   #01.1
//   │    general-purpose / claude-opus-5-5 · shiny-kite · …000a working
//   │    » read the mail store and list the open threads
//   │    ▸ Read: ~/.aoide/state/mail/index.json
//   │    Reading the index; three threads are open, the newest from…
//   │    …
//   └─ ◐ subagent: audit the fixtures                    up  4m   #01.2
//        …                                                (+ the summons lane)
//        ! quick-wren wants to run a command        [approve] [deny]
//
// A main card is 9 lines, a subagent's 6 — fixed, so streaming data never
// shifts the silhouette. The one line that comes and goes on its own is the
// SUMMONS lane: it exists only while a permission summons for that session
// stands in herald.json (sonata's rule: the ledger, not `awaiting`, proves a
// verdict can be delivered). The selected card also grows its action line.
//
//   ● rook-lantern · Bash: cargo test          working   [2]  1h02m     tty
//     ~/Aoide                                              rook-lantern
//
// A terminal card is 2 lines: agent (the title, else the harness; a bare
// shell shows its live command, else "shell") · state · jack · elapsed, then
// the cwd. One card per window: the live agent in it wins (terminals.qml).
//
// A HELPER (uppercase — never a slot). Takes `kit`, `board` (BoardBody: its
// `cards` model, clock, actions), `what` ("agents" | "terms") and `project`
// ("" = every project, grouped; else only that project's cards).
//
// ── Actions (the bridge only) ─────────────────────────────────────────────
//   click a card        bridge.focusSession — a subagent focuses its parent's
//                       window (sonata's courier rule)
//   [approve] / [deny]  {cmd:"heraldverdict", id:<sessionId>, verdict} — the
//                       NOTIF tab's and the herald's own wire line; click only
//   right-click / j k   select; the selected card's action line carries
//                       sonata SessionMenu's sessionAction project / undying /
//                       kill (kill confirms y/N, refused for subagents)
// Every published string (title, prompt, say, tool, summons summary, cwd) is
// untrusted: Text.PlainText, control and bidi characters flattened, one line
// or a fixed box, elided (house rule 4). Nothing here is ever actionable text.
//
// ── Motion ────────────────────────────────────────────────────────────────
// Only the lamps of working and awaiting cards move: they breathe (opacity
// 0.45 → 1, 2s cosine) off ONE shared phase, stepped at 12.5 fps by one Timer
// that runs only while the board is open and a shown card is working or
// awaiting. Everything else is still.
import QtQuick

Item {
    id: root

    required property var kit
    required property var board
    property string what: "agents"
    property string project: ""

    readonly property var cards: board.cards
    readonly property int w: kit.fit(width)

    // ══ untrusted text → one plain line ═══════════════════════════════════
    function flat(v) {
        if (v === null || v === undefined) return ""
        return ("" + v).replace(/\u001b\[[0-9;?]*[ -\/]*[@-~]/g, "")
                        .replace(/[\u0000-\u001f\u007f-\u009f\u200b-\u200f\u2028-\u202e\u2066-\u2069\s]+/g, " ").trim()
    }
    function shortId(id) {
        var v = "" + (id || "")
        return v.length > 4 ? "…" + v.slice(-4) : v
    }
    // sonata's cwd breadcrumb: ~, the first segment, …, the last two
    function crumb(p) {
        var s = board.shortPath(root.flat(p))
        if (!s) return ""
        var tilde = s.charAt(0) === "~", abs = s.charAt(0) === "/"
        var parts = s.split("/").filter(function (x) { return x.length > 0 })
        if (tilde) parts.shift()
        if (parts.length > 3)
            return (tilde ? "~/" : (abs ? "/" : "")) + parts[0] + "/…/" + parts.slice(parts.length - 2).join("/")
        return s
    }
    // elapsed since `iso`, at the board clock's 30s grain (no seconds past 1m)
    function up(iso) {
        if (!iso) return "—"
        var t = Date.parse("" + iso)
        if (isNaN(t)) return "—"
        var d = Math.max(0, Math.floor((board.nowMs - t) / 1000))
        if (d < 60) return "<1m"
        var m = Math.floor(d / 60)
        if (m < 60) return m + "m"
        var h = Math.floor(m / 60), mm = m % 60
        if (h < 24) return h + "h" + (mm < 10 ? "0" : "") + mm + "m"
        var dd = Math.floor(h / 24), hh = h % 24
        return dd + "d" + (hh < 10 ? "0" : "") + hh + "h"
    }
    function compact(n) {
        var lv = board.livery
        if (lv && lv.ctxCompact) return lv.ctxCompact(n)
        var v = n || 0
        if (v < 1000) return "" + v
        if (v < 1000000) return Math.round(v / 1000) + "k"
        return (v / 1000000).toFixed(1) + "M"
    }
    function ctxPct(s) {
        var lv = board.livery
        if (lv && lv.ctxPercent) return lv.ctxPercent(s.contextTokens, s.contextCeiling)
        var c = s.contextCeiling > 0 ? s.contextCeiling : 200000
        return Math.max(0, Math.min(100, (s.contextTokens || 0) / c * 100))
    }
    readonly property real ctxUrgentAt: (board.livery && board.livery.ctxUrgentAt) ? board.livery.ctxUrgentAt : 85

    // the live state: the hook phase wins, the roster state is the fallback
    function liveState(s) {
        var h = s ? root.cards.hookBy[s.sessionId] : null
        return (h && h.phase) ? h.phase : ((s && s.state) ? ("" + s.state).toLowerCase() : "")
    }
    function lampColor(s) {
        if (s && s.sessionId === board.hotId) return kit.hot
        if (s && s.needsSudo) return kit.urgent
        return kit.lampColor(root.liveState(s))
    }
    function summonsOf(s) { return (s && s.sessionId) ? (root.cards.summonsBy[s.sessionId] || null) : null }
    function focusCard(s, child) {
        if (!s || !board.bridge || !board.bridge.focusSession) return
        var id = (child && s.parentSessionId) ? s.parentSessionId : s.sessionId
        if (id) board.bridge.focusSession(id)
    }
    // the HAND lane (sonata's toolText without the trace poll): the rich
    // reaper label when it names the same call, else the live hook name
    function toolText(s) {
        var live = root.flat(s.activity), last = root.flat(s.tool)
        if (live === "") return last
        var a = live.toLowerCase(), b = last.toLowerCase()
        return (b === a || b.indexOf(a + ":") === 0) ? last : live
    }
    function thinkText(s) {
        var said = root.flat(s.say)
        if (said !== "") return said
        var h = root.cards.hookBy[s.sessionId]
        return h ? h.phase + " …" : "…"
    }
    function provenance(s) {
        var agent = root.flat(s.agent) || "agent"
        var title = root.flat(s.title)
        return (s.model || (title && title !== agent)) ? agent + (s.model ? " / " + root.flat(s.model) : "") : ""
    }

    // ══ SELECTION + the action line (sonata SessionMenu's three actions) ═══
    property string selId: ""
    property string mode: ""            // "" | "project" | "kill"
    property string msg: ""
    property bool msgFailed: false
    property bool busy: false
    readonly property var selectable: {
        var out = []
        for (var i = 0; i < root.entries.length; i++)
            if (root.entries[i].type === "card") out.push(root.entries[i].row.s)
        return out
    }
    readonly property int selIndex: {
        for (var i = 0; i < selectable.length; i++) if (selectable[i].sessionId === selId) return i
        return -1
    }
    readonly property var selS: selIndex >= 0 ? selectable[selIndex] : null
    function select(i) {
        if (selectable.length === 0) return
        i = Math.max(0, Math.min(selectable.length - 1, i))
        if (selectable[i].sessionId !== selId) { selId = selectable[i].sessionId; mode = ""; msg = "" }
    }
    function killable(s) {
        return !!s && s.kind !== "app" && !board.nested(s)
            && ("" + (s.sessionId || "")).indexOf("sub:") !== 0 && board.isLive(s)
    }
    function isUndying(s) { return !!s && board.undyingIds.indexOf(s.sessionId) >= 0 }
    function run(action, fields) {
        var s = root.selS
        if (!s || busy) return
        if (!board.bridge || !board.bridge.sessionAction) { msgFailed = true; msg = "session actions unavailable"; return }
        busy = true; msgFailed = false; msg = "waiting for aoide…"; mode = ""
        board.bridge.sessionAction(s.sessionId, action, fields, function (r) {
            root.busy = false
            root.msgFailed = !(r && r.ok)
            root.msg = (r && r.message) ? root.flat(r.message) : (root.msgFailed ? "action failed" : "done")
        })
    }
    function cancel() {
        if (mode !== "") { mode = ""; return true }
        if (selId !== "") { selId = ""; msg = ""; return true }
        return false
    }
    function handleKey(e) {
        if (what !== "agents") return false
        var s = root.selS
        var projects = board.model.projects
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
            if (s) focusCard(s, board.nested(s)); return true
        case Qt.Key_P: if (s) { mode = "project"; msg = "" } return true
        case Qt.Key_U: if (s) run("undying", { state: isUndying(s) ? "off" : "on" }); return true
        case Qt.Key_X: if (killable(s)) { mode = "kill"; msg = "" } return true
        }
        return false
    }

    // ══ THE FLAT LIST — every line counted here, so the pane is whole cells ═
    readonly property int mainLines: 9
    readonly property int subLines: 6
    readonly property var entries: {
        var out = []
        if (root.what === "terms") {
            var ts = root.cards.ttys
            for (var t = 0; t < ts.length; t++)
                if (root.project === "" || ts[t].project === root.project)
                    out.push({ type: "tty", tty: ts[t], lines: 2 })
            return out
        }
        var gs = root.cards.groups
        var chrome = root.project === ""
        for (var i = 0; i < gs.length; i++) {
            var g = gs[i]
            if (!chrome && !(g.anchored && g.name === root.project)) continue
            if (chrome && g.name !== "") out.push({ type: "head", g: g, lines: 1 })
            for (var r = 0; r < g.rows.length; r++) {
                var row = g.rows[r]
                if (r > 0) out.push({ type: "sep", lines: 1 })
                var place = g.anchored ? g.name : ""
                out.push(root.cardEntry(row, false, false, place))
                for (var k = 0; k < row.kids.length; k++)
                    out.push(root.cardEntry(row.kids[k], true, k === row.kids.length - 1, place))
            }
        }
        return out
    }
    function cardEntry(row, child, last, place) {
        var n = child ? root.subLines : root.mainLines
        if (root.summonsOf(row.s)) n++
        if (row.s.sessionId === root.selId) n++
        return { type: "card", row: row, child: child, last: last, place: place, lines: n }
    }
    readonly property int lineCount: {
        var n = 0
        for (var i = 0; i < entries.length; i++) n += entries[i].lines
        return Math.max(1, n)
    }
    implicitHeight: kit.lines(lineCount)
    height: implicitHeight

    // ══ THE BREATH — one phase for every moving lamp ═════════════════════
    readonly property bool anyMoving: {
        for (var i = 0; i < entries.length; i++) {
            var e = entries[i]
            var s = e.type === "card" ? e.row.s : (e.type === "tty" ? e.tty.s : null)
            if (!s) continue
            var st = root.liveState(s)
            if (st === "working" || st === "awaiting") return true
        }
        return false
    }
    property real phase: 0
    readonly property real breath: 0.5 - 0.5 * Math.cos(2 * Math.PI * phase)
    Timer {
        interval: 80; repeat: true
        running: root.anyMoving && root.board.open && root.visible
        onTriggered: root.phase = (root.phase + 0.04) % 1
        onRunningChanged: if (!running) root.phase = 0
    }
    function lampOpacity(s) {
        var st = root.liveState(s)
        return (st === "working" || st === "awaiting") ? 0.45 + 0.55 * (1 - root.breath) : 1
    }

    // ══ PAINT ══════════════════════════════════════════════════════════════
    Text {
        visible: root.entries.length === 0
        text: root.what === "terms"
              ? (root.project ? "no terminals in " + root.project : "no conducted terminals")
              : (root.project ? "no live agents in " + root.project : "no live agents")
        color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText
    }
    Column {
        width: root.width
        Repeater {
            model: root.entries
            Loader {
                required property var modelData
                readonly property var entry: modelData
                width: root.width
                height: root.kit.lines(modelData.lines)
                sourceComponent: modelData.type === "card" ? agentCard
                               : modelData.type === "tty" ? ttyCard
                               : modelData.type === "head" ? groupHead : sepLine
            }
        }
    }

    // ── a separator between two main cards: a hairline mid-line ──────────
    Component {
        id: sepLine
        Item {
            width: parent ? parent.width : 0; height: root.kit.cellH
            Rectangle {
                y: Math.round(root.kit.cellH / 2); width: parent.width; height: 1
                color: root.kit.withA(root.kit.dim, 0.5)
            }
        }
    }

    // ── a project group's head: name, rule, tallies ──────────────────────
    Component {
        id: groupHead
        Item {
            id: gh
            readonly property var g: parent ? parent.entry.g : null
            width: parent ? parent.width : 0; height: root.kit.cellH
            Text {
                id: ghName
                text: gh.g ? root.flat(gh.g.name) : ""
                color: gh.g && gh.g.anchored ? root.kit.path : root.kit.dim
                font: root.kit.titleFont; textFormat: Text.PlainText
                style: Text.Outline; styleColor: root.kit.withA(color, 0.18)
            }
            Rectangle {
                x: ghName.implicitWidth + root.kit.cellW
                y: Math.round(root.kit.cellH / 2)
                width: Math.max(0, ghTally.x - x - root.kit.cellW); height: 1
                color: root.kit.dim
            }
            Row {
                id: ghTally
                anchors.right: parent.right
                Text { text: gh.g ? "" + gh.g.agents : ""; color: root.kit.number; font: root.kit.font; textFormat: Text.PlainText }
                Text { text: " agt · "; color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText }
                Text { text: gh.g ? "" + gh.g.working : ""; color: root.kit.number; font: root.kit.font; textFormat: Text.PlainText }
                Text { text: " working · "; color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText }
                Text { text: gh.g ? root.compact(gh.g.tokens) : ""; color: root.kit.number; font: root.kit.font; textFormat: Text.PlainText }
                Text { text: " tok"; color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText }
            }
        }
    }

    // ── ONE AGENT CARD (main: 9 lines · subagent: 6), + summons, + action ─
    Component {
        id: agentCard
        Item {
            id: card
            readonly property var e: parent ? parent.entry : null
            readonly property var s: e ? e.row.s : ({})
            readonly property bool child: !!(e && e.child)
            readonly property string no: e ? (card.child ? "#" + e.row.no
                                                          : "#" + (e.row.no.length < 2 ? "0" : "") + e.row.no) : ""
            readonly property string st: root.liveState(s)
            readonly property var summons: root.summonsOf(s)
            readonly property bool sel: !!s && s.sessionId === root.selId
            readonly property bool hasWs: s.workspace !== undefined && s.workspace !== null
            readonly property int lead: child ? 3 : 0             // the tree gutter, cells
            readonly property int textX: root.kit.cells(lead + 2)  // past the lamp
            readonly property int bodyLines: child ? root.subLines : root.mainLines
            readonly property int thinkLines: child ? 2 : 3
            readonly property bool hasCtx: (s.contextTokens || 0) > 0
            width: parent ? parent.width : 0
            height: root.kit.lines(e ? e.lines : 1)

            // the whole card focuses its window; right-click selects it
            MouseArea {
                anchors.fill: parent
                acceptedButtons: Qt.LeftButton | Qt.RightButton
                cursorShape: Qt.PointingHandCursor
                onClicked: function (m) {
                    if (m.button === Qt.RightButton) {
                        if (root.selId === card.s.sessionId) root.selId = ""
                        else { root.selId = card.s.sessionId; root.mode = ""; root.msg = "" }
                        root.board.forceActiveFocus()
                    } else root.focusCard(card.s, card.child)
                }
            }

            // the tree limbs down the gutter (subagents)
            Repeater {
                model: card.child && card.e ? card.e.lines : 0
                Text {
                    required property int index
                    y: root.kit.lines(index)
                    text: index === 0 ? (card.e.last ? "└─" : "├─") : (card.e.last ? "" : "│")
                    color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText
                }
            }

            // ── 1 · IDENTITY: lamp · title ········ [ws] #NN (sub: up · #NN.k)
            Rectangle {
                visible: card.sel
                x: root.kit.cells(card.lead); width: parent.width - x; height: root.kit.cellH
                color: root.kit.select
            }
            Text {
                x: root.kit.cells(card.lead)
                text: root.kit.lampGlyph(card.st)
                color: root.lampColor(card.s)
                opacity: root.lampOpacity(card.s)
                font: root.kit.font; textFormat: Text.PlainText
            }
            Text {
                x: card.textX
                width: Math.max(0, idRight.x - x - root.kit.cellW)
                text: root.flat(card.s.title) || root.flat(card.s.agent) || (card.child ? "subagent" : "agent")
                elide: Text.ElideRight
                color: card.sel ? root.kit.match : root.kit.bright
                font: root.kit.titleFont; textFormat: Text.PlainText
                style: Text.Outline; styleColor: root.kit.withA(color, 0.18)
            }
            Row {
                id: idRight
                anchors.right: parent.right
                Text {
                    visible: card.child
                    text: "up "; color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText
                }
                Text {
                    visible: card.child
                    text: root.kit.padL(root.up(card.s.startedAt), 5) + "  "
                    color: root.kit.number; font: root.kit.font; textFormat: Text.PlainText
                }
                Text {
                    visible: !card.child
                    text: card.hasWs ? "[" + card.s.workspace + "]  " : ""
                    color: root.kit.path; font: root.kit.font; textFormat: Text.PlainText
                }
                Text {
                    text: card.no
                    color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText
                }
            }

            // ── 2 · PROVENANCE: harness / model (sub: + place) ······· state
            Text {
                x: card.textX; y: root.kit.lines(1)
                width: Math.max(0, stateWord.x - x - root.kit.cellW)
                text: {
                    var p = root.provenance(card.s)
                    if (!card.child) return p
                    var bits = [p, root.flat(card.s.petname), root.shortId(card.s.sessionId)]
                    return bits.filter(function (b) { return b !== "" }).join(" · ")
                }
                elide: Text.ElideRight
                color: root.kit.mid; font: root.kit.font; textFormat: Text.PlainText
            }
            Text {
                id: stateWord
                anchors.right: parent.right; y: root.kit.lines(1)
                text: (card.s.needsSudo ? "sudo · " : "") + (card.st || "—")
                color: card.s.needsSudo || card.st === "awaiting" ? root.kit.urgent : root.kit.lampColor(card.st)
                font: root.kit.font; textFormat: Text.PlainText
            }

            // ── 3 · PLACE (mains): petname · …id · host · project ──────────
            Row {
                visible: !card.child
                x: card.textX; y: root.kit.lines(2)
                Text {
                    text: root.flat(card.s.petname)
                    color: root.kit.ink; font: root.kit.font; textFormat: Text.PlainText
                }
                Text {
                    text: (root.flat(card.s.petname) ? " · " : "") + root.shortId(card.s.sessionId)
                          + (root.flat(card.s.host) ? " · " + root.flat(card.s.host) : "")
                    color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText
                }
                Text {
                    text: card.e && card.e.place ? " · " + card.e.place : ""
                    color: root.kit.path; font: root.kit.font; textFormat: Text.PlainText
                }
            }

            // ── 4 · DIRECTIVE: » prompt ───────────────────────────────────
            Text {
                x: card.textX; y: root.kit.lines(card.child ? 2 : 3)
                width: Math.max(0, card.width - x)
                readonly property string p: root.flat(card.s.prompt)
                text: "» " + (p || "—")
                elide: Text.ElideRight
                color: p ? root.kit.ink : root.kit.dim
                font: root.kit.font; textFormat: Text.PlainText
            }

            // ── 5 · HAND: ▸ the tool, one line ────────────────────────────
            Text {
                x: card.textX; y: root.kit.lines(card.child ? 3 : 4)
                width: Math.max(0, card.width - x)
                readonly property string t: root.toolText(card.s)
                text: "▸ " + (t || "—")
                elide: Text.ElideRight
                color: t ? root.kit.ink : root.kit.dim
                font: root.kit.font; textFormat: Text.PlainText
            }

            // ── 6 · THINKING: a fixed box, 3 lines main / 2 sub; the last elides
            Text {
                x: card.textX; y: root.kit.lines(card.child ? 4 : 5)
                width: Math.max(0, card.width - x)
                height: root.kit.lines(card.thinkLines)
                readonly property bool said: root.flat(card.s.say) !== ""
                text: root.thinkText(card.s)
                wrapMode: Text.Wrap
                maximumLineCount: card.thinkLines
                elide: Text.ElideRight
                clip: true
                lineHeightMode: Text.FixedHeight; lineHeight: root.kit.cellH
                color: said ? root.kit.mid : root.kit.dim
                font: root.kit.font; textFormat: Text.PlainText
            }

            // ── 7 · SUMMONS: only while one stands in herald.json ─────────
            Item {
                visible: !!card.summons
                x: card.textX; y: root.kit.lines(card.bodyLines - (card.child ? 0 : 1))
                width: Math.max(0, card.width - x); height: root.kit.cellH
                Text {
                    width: Math.max(0, verdicts.x - root.kit.cellW)
                    text: "! " + (card.summons ? (root.flat(card.summons.summary) || "permission") : "")
                    elide: Text.ElideRight
                    color: root.kit.urgent; font: root.kit.font; textFormat: Text.PlainText
                }
                Row {
                    id: verdicts
                    anchors.right: parent.right
                    Text { text: "["; color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText }
                    Text {
                        text: "approve"; color: root.kit.match; font: root.kit.font; textFormat: Text.PlainText
                        MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor
                                    onClicked: root.board.heraldVerdict(card.s.sessionId, "approve") }
                    }
                    Text { text: "] ["; color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText }
                    Text {
                        text: "deny"; color: root.kit.urgent; font: root.kit.font; textFormat: Text.PlainText
                        MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor
                                    onClicked: root.board.heraldVerdict(card.s.sessionId, "deny") }
                    }
                    Text { text: "]"; color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText }
                }
            }

            // ── 8 · PULSE + GROUND (mains): cwd ········ ctx gauge · up ────
            Item {
                visible: !card.child
                x: card.textX
                y: root.kit.lines(card.bodyLines - 1 + (card.summons ? 1 : 0))
                width: Math.max(0, card.width - x); height: root.kit.cellH
                readonly property string cwd: root.crumb(card.s.cwd)
                Text {
                    width: Math.max(0, pulse.x - root.kit.cellW)
                    text: parent.cwd || "—"
                    elide: Text.ElideLeft
                    color: parent.cwd ? root.kit.path : root.kit.dim
                    font: root.kit.font; textFormat: Text.PlainText
                }
                Row {
                    id: pulse
                    anchors.right: parent.right
                    readonly property real pct: card.hasCtx ? root.ctxPct(card.s) : 0
                    readonly property var g: root.kit.gaugeText(pct / 100, 8)
                    Text { text: "ctx "; color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText }
                    Text {
                        visible: card.hasCtx
                        text: pulse.g.fill
                        color: pulse.pct >= root.ctxUrgentAt ? root.kit.urgent : root.kit.ink
                        font: root.kit.font; textFormat: Text.PlainText
                    }
                    Text {
                        visible: card.hasCtx
                        text: pulse.g.track + " "
                        color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText
                    }
                    Text {
                        text: card.hasCtx
                              ? root.compact(card.s.contextTokens) + (card.s.contextCeiling ? "/" + root.compact(card.s.contextCeiling) : "")
                              : "—"
                        color: card.hasCtx ? root.kit.number : root.kit.dim
                        font: root.kit.font; textFormat: Text.PlainText
                    }
                    Text { text: "  up "; color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText }
                    Text {
                        text: root.up(card.s.startedAt)
                        color: root.kit.number; font: root.kit.font; textFormat: Text.PlainText
                    }
                }
            }

            // ── the selected card's action line ───────────────────────────
            Item {
                visible: card.sel
                x: root.kit.cells(card.lead); y: card.height - root.kit.cellH
                width: Math.max(0, card.width - x); height: root.kit.cellH
                Rectangle { anchors.fill: parent; color: root.kit.select }
                Row {
                    x: root.kit.cells(2)
                    visible: root.mode === "" && root.msg === ""
                    Repeater {
                        model: [
                            { k: "↵", t: "focus",   on: true,  a: "focus" },
                            { k: "p", t: "project", on: true,  a: "project" },
                            { k: "u", t: root.isUndying(card.s) ? "undying ✓" : "undying", on: true, a: "undying" },
                            { k: "x", t: root.killable(card.s) ? "kill" : "kill n/a", on: root.killable(card.s), a: "kill" }
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
                                        if (a === "focus") root.focusCard(card.s, card.child)
                                        else if (a === "project") root.mode = "project"
                                        else if (a === "undying") root.run("undying", { state: root.isUndying(card.s) ? "off" : "on" })
                                        else if (a === "kill") root.mode = "kill"
                                    }
                                }
                            }
                        }
                    }
                }
                Row {
                    x: root.kit.cells(2)
                    visible: root.mode === "project"
                    Text { text: "project: "; color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText }
                    Repeater {
                        model: [{ n: 0, name: "" }].concat(root.board.model.projects.slice(0, 9).map(function (p, i) { return { n: i + 1, name: p.name } }))
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
                Text {
                    x: root.kit.cells(2)
                    visible: root.mode === "kill"
                    text: "kill " + (root.flat(card.s.petname) || card.s.sessionId) + "?  [y/N]"
                    color: root.kit.urgent; font: root.kit.font; textFormat: Text.PlainText
                }
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

    // ── ONE TERMINAL CARD (2 lines) ───────────────────────────────────────
    Component {
        id: ttyCard
        Item {
            id: tc
            readonly property var t: parent ? parent.entry.tty : null
            readonly property var s: t ? t.s : ({})
            readonly property bool agentRow: root.board.kindOf(s) !== "shell"
            readonly property string st: root.liveState(s)
            readonly property bool hasWs: s.workspace !== undefined && s.workspace !== null
            width: parent ? parent.width : 0
            height: root.kit.lines(2)
            MouseArea {
                anchors.fill: parent
                cursorShape: Qt.PointingHandCursor
                onClicked: root.focusCard(tc.s, false)
            }
            Text {
                text: root.kit.lampGlyph(tc.st)
                color: root.lampColor(tc.s)
                opacity: root.lampOpacity(tc.s)
                font: root.kit.font; textFormat: Text.PlainText
            }
            Text {
                x: root.kit.cells(2)
                width: Math.max(0, ttyRight.x - x - root.kit.cellW)
                text: tc.agentRow ? (root.flat(tc.s.title) || root.flat(tc.s.agent) || "agent")
                                  : (root.flat(tc.s.activity) || root.flat(tc.s.agent) || "shell")
                elide: Text.ElideRight
                color: tc.agentRow ? root.kit.bright : root.kit.ink
                font: root.kit.font; textFormat: Text.PlainText
                style: Text.Outline; styleColor: root.kit.withA(color, 0.18)
            }
            Row {
                id: ttyRight
                anchors.right: parent.right
                Text {
                    text: (tc.st || "—") + "  "
                    color: tc.st === "awaiting" ? root.kit.urgent : root.kit.lampColor(tc.st)
                    font: root.kit.font; textFormat: Text.PlainText
                }
                Text {
                    text: root.kit.padR(tc.hasWs ? "[" + tc.s.workspace + "]" : "[·]", 5)
                    color: tc.hasWs ? root.kit.path : root.kit.dim
                    font: root.kit.font; textFormat: Text.PlainText
                }
                Text {
                    text: root.kit.padL(root.up(tc.s.startedAt), 6)
                    color: root.kit.number; font: root.kit.font; textFormat: Text.PlainText
                }
            }
            Text {
                x: root.kit.cells(2); y: root.kit.cellH
                width: Math.max(0, ttyPet.x - x - root.kit.cellW)
                readonly property string cwd: root.board.shortPath(root.flat(tc.s.cwd))
                text: cwd || "—"
                elide: Text.ElideLeft
                color: cwd ? root.kit.path : root.kit.dim
                font: root.kit.font; textFormat: Text.PlainText
            }
            Text {
                id: ttyPet
                anchors.right: parent.right; y: root.kit.cellH
                text: [root.flat(tc.s.petname), tc.t && tc.t.project ? tc.t.project : ""].filter(function (b) { return b !== "" }).join(" · ")
                color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText
            }
        }
    }
}
