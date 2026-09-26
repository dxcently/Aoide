// BoardBody.qml — the BOARD: cadenza's dock body (intent §3.3). Everything
// the board draws lives here and in the four Board* views it loads; dock.qml
// is only the thin PanelWindow shell around it (the canvas refuses window
// roots, so this Item is what `lyra preview` shows).
//
//   ┌─ BOARD ─────────────────────────────────────────── 12 live ┐
//   │ OVERVIEW │ aoide │ melete │ mneme │ SYS │ NOTIF !1          │
//   │ ─────────────────────────────────────────────────────────── │
//   │  (the tab's view: BoardOverview / BoardProject / BoardSys /  │
//   │   BoardNotif, each loaded BY URL — design/kit.md §1)         │
//   └─────────────────────────────────────────────────────────────┘
//
// A HELPER (uppercase — never a slot).
//
// ── What it reads (every path is a documented contract) ──────────────────
//   state/stage/sessions.json · hooks.json · projects.json · herald.json
//   state/stage/graph.json        `workspaces[].project` — the S1 binding
//   livery.usagePath              state/usage.json, the account block
//   state/undying.json            the undying mark (SessionMenu's read)
//   state/usage/now.json          §C per-jack numbers — absent until S5/S6
// Stage/state dirs resolve exactly as sonata's SessionMenu does
// (AOIDE_STAGE_DIR / AOIDE_STATE_DIR, else under AOIDE_ROOT).
//
// ── What it cannot read yet (honest empty, intent §3.3's table) ──────────
// The board feed and the mail view come from `aoide project board` through
// a shellbridge read op that does not exist (S8–S10). The live adapter will
// set `boardWired` + `boards`; until then both stay empty and the views say
// `no feed — bridge not wired` / `no mail view — bridge not wired`. Posting
// (`boardpost`, S11/S12) is not wired: the composer is drawn disabled.
// BoardPreview.qml feeds the fixture answer through these SAME properties;
// this file never reads a fixture path.
//
// ── Actions (house rule 7: QML paints, the bridge acts) ──────────────────
//   bridge.focusSession(id)                 a row / rail click
//   bridge.sessionAction(id, a, fields, cb) project · undying · kill, as
//                                           sonata's SessionMenu sends them
//   bridge.sendCommand({cmd:"heraldverdict", id, verdict})   a summons
//   bridge.sendCommand({cmd:"heralddismiss", id})            a toast / "*"
//   bridge.refreshUsage()                   SYS account [r]
// Every board/herald/session text is Text.PlainText and never pre-fills
// the composer (house rule 4).
import QtQuick
import Quickshell
import Quickshell.Io
import "Kit.js" as Kit

Item {
    id: board

    required property var livery
    required property var bridge
    property var shared: null
    property var stagingEngine: null

    // ── the board API (dock.qml forwards toggle/openTab/open to these) ────
    property bool open: true
    property string tab: "overview"
    signal closeRequested()

    // ── unbuilt seams: the live adapter sets these once each slice lands ──
    property bool boardWired: false     // S8–S10: the project board read op
    property var boards: ({})           // project name → §D board answer
    property var usageNow: null         // §C now.json (FileView below, or a feeder)

    // ── the kit: one per widget ───────────────────────────────────────────
    FontMetrics { id: fm; font: Kit.bodyFont(13) }
    readonly property var kit: Kit.make(board.livery, fm)

    implicitWidth: kit.cells(72)
    implicitHeight: kit.lines(100)
    width: parent ? parent.width : implicitWidth
    height: parent ? parent.height : implicitHeight

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

    // ══ PATHS ═════════════════════════════════════════════════════════════
    readonly property string rootDir: Quickshell.env("AOIDE_ROOT") || (Quickshell.env("HOME") + "/.aoide")
    readonly property string stateDir: {
        var p = Quickshell.env("AOIDE_STATE_DIR") || ""
        return p.charAt(0) === "/" ? p : board.rootDir + "/state"
    }
    readonly property string stageDir: {
        var p = Quickshell.env("AOIDE_STAGE_DIR") || ""
        return p.charAt(0) === "/" ? p : board.stateDir + "/stage"
    }

    // ══ RAW STATE (plain values; each FileView owns one) ══════════════════
    property var sessions: []
    property var hooks: []
    property var projects: []
    property var heraldRecs: []
    property var graphWorkspaces: []
    property var account: null
    property var undyingIds: []

    function _json(fv) {
        try {
            var t = fv.text()
            if (!t || t.trim().length === 0) return null
            return JSON.parse(t)
        } catch (e) { return null }
    }

    FileView {
        id: sessionsFile
        path: board.stageDir + "/sessions.json"
        watchChanges: true; blockLoading: false; printErrors: false
        onFileChanged: reload()
        onLoaded: { var o = board._json(sessionsFile); board.sessions = (o && o.sessions) ? o.sessions : [] }
        onLoadFailed: board.sessions = []
    }
    FileView {
        id: hooksFile
        path: board.stageDir + "/hooks.json"
        watchChanges: true; blockLoading: false; printErrors: false
        onFileChanged: reload()
        onLoaded: { var o = board._json(hooksFile); board.hooks = (o && o.hooks) ? o.hooks : [] }
        onLoadFailed: board.hooks = []
    }
    FileView {
        id: projectsFile
        path: board.stageDir + "/projects.json"
        watchChanges: true; blockLoading: false; printErrors: false
        onFileChanged: reload()
        onLoaded: { var o = board._json(projectsFile); board.projects = (o && o.projects) ? o.projects : [] }
        onLoadFailed: board.projects = []
    }
    FileView {
        id: heraldFile
        path: board.stageDir + "/herald.json"
        watchChanges: true; blockLoading: false; printErrors: false
        onFileChanged: reload()
        onLoaded: { var o = board._json(heraldFile); board.heraldRecs = (o && o.notifications) ? o.notifications : [] }
        onLoadFailed: board.heraldRecs = []
    }
    FileView {
        id: graphFile
        path: board.stageDir + "/graph.json"
        watchChanges: true; blockLoading: false; printErrors: false
        onFileChanged: reload()
        onLoaded: { var o = board._json(graphFile); board.graphWorkspaces = (o && o.workspaces) ? o.workspaces : [] }
        onLoadFailed: board.graphWorkspaces = []
    }
    FileView {
        id: accountFile
        path: board.livery ? board.livery.usagePath : ""
        watchChanges: true; blockLoading: false; printErrors: false
        onFileChanged: reload()
        onLoaded: board.account = board._json(accountFile)
        onLoadFailed: board.account = null
    }
    FileView {
        id: undyingFile
        path: board.stateDir + "/undying.json"
        watchChanges: true; blockLoading: false; printErrors: false
        onFileChanged: reload()
        onLoaded: {
            var o = board._json(undyingFile)
            var list = o ? (o.undying || o.carried || []) : []
            board.undyingIds = list.map(function (r) { return r.sessionId })
        }
        onLoadFailed: board.undyingIds = []
    }
    // §C: the per-workspace usage row set. Only a successful parse assigns,
    // so a feeder (BoardPreview) is not blanked by an absent file; a file
    // that was read and then vanishes clears what it put there.
    property bool _nowFromFile: false
    FileView {
        id: nowFile
        path: board.stateDir + "/usage/now.json"
        watchChanges: true; blockLoading: false; printErrors: false
        onFileChanged: reload()
        onLoaded: {
            var o = board._json(nowFile)
            if (o && o.rows) { board.usageNow = o; board._nowFromFile = true }
        }
        onLoadFailed: if (board._nowFromFile) { board.usageNow = null; board._nowFromFile = false }
    }

    // ══ CLOCK — a 30s tick for ages, only while open (at rest nothing moves)
    property real nowMs: Date.now()
    Timer {
        interval: 30000; repeat: true; running: board.open
        triggeredOnStart: true
        onTriggered: board.nowMs = Date.now()
    }
    function age(iso) {
        if (!iso) return ""
        var t = Date.parse("" + iso)
        if (isNaN(t)) return ""
        var s = Math.max(0, Math.floor((board.nowMs - t) / 1000))
        if (s < 5) return "now"
        if (s < 60) return s + "s"
        if (s < 3600) return Math.floor(s / 60) + "m"
        if (s < 86400) return Math.floor(s / 3600) + "h"
        return Math.floor(s / 86400) + "d"
    }
    function hhmm(iso) {
        var d = new Date("" + iso)
        if (isNaN(d.getTime())) return "--:--"
        var h = d.getHours(), m = d.getMinutes()
        return (h < 10 ? "0" : "") + h + ":" + (m < 10 ? "0" : "") + m
    }
    function shortPath(p) {
        if (!p) return ""
        var s = "" + p
        var home = Quickshell.env("HOME")
        if (home && s.indexOf(home) === 0) s = "~" + s.substring(home.length)
        return s
    }
    function human(n) {
        if (n === null || n === undefined || isNaN(n)) return "—"
        var a = Math.abs(n)
        if (a >= 1e9) return (n / 1e9).toFixed(1) + "G"
        if (a >= 1e6) return (n / 1e6).toFixed(1) + "M"
        if (a >= 1e3) return (n / 1e3).toFixed(1) + "k"
        return "" + Math.round(n)
    }
    function bytes(n) {
        if (n === null || n === undefined || isNaN(n)) return "—"
        if (n >= 1073741824) return (n / 1073741824).toFixed(1) + "G"
        if (n >= 1048576) return Math.round(n / 1048576) + "M"
        return Math.round(n / 1024) + "k"
    }

    // ══ THE MODEL — derived, never stored anywhere but here ═══════════════
    function kindOf(s) {
        if (s && s.kind) return ("" + s.kind).toLowerCase()
        return (s && s.agent === "shell") ? "shell" : "agent"
    }
    function isLive(s) {
        return !!s && (s.state === "working" || s.state === "awaiting"
                       || s.state === "stopped" || s.state === "idle")
    }
    function projectOfCwd(cwd, ps) {
        if (!cwd) return ""
        var best = "", bestLen = -1
        for (var i = 0; i < ps.length; i++) {
            var roots = ps[i].roots && ps[i].roots.length ? ps[i].roots : [ps[i].path]
            for (var j = 0; j < roots.length; j++) {
                var p = roots[j]
                if (!p) continue
                if ((cwd === p || cwd.indexOf(p === "/" ? "/" : p + "/") === 0) && p.length > bestLen) {
                    best = ps[i].name || ""; bestLen = p.length
                }
            }
        }
        return best
    }
    // explicit project > the nearest ancestor's > the cwd anchor
    function effProject(s, byId, ps) {
        var cur = s, hops = 0
        while (cur && hops < 8) {
            if (cur.project) return "" + cur.project
            var own = projectOfCwd(cur.cwd, ps)
            if (own) return own
            cur = cur.parentSessionId ? byId[cur.parentSessionId] : null
            hops++
        }
        return ""
    }
    function byStart(a, b) {
        var x = (a.s && a.s.startedAt) || "", y = (b.s && b.s.startedAt) || ""
        return x < y ? -1 : (x > y ? 1 : 0)
    }

    readonly property var model: {
        var all = board.sessions || [], ps = board.projects || []
        var hookBy = {}
        for (var h = 0; h < (board.hooks || []).length; h++) {
            var hk = board.hooks[h]
            if (hk && hk.sessionId) hookBy[hk.sessionId] = hk
        }
        var byId = {}
        for (var i = 0; i < all.length; i++) if (all[i] && all[i].sessionId) byId[all[i].sessionId] = all[i]

        var tops = [], kids = {}, terms = []
        for (i = 0; i < all.length; i++) {
            var s = all[i]
            if (!board.isLive(s)) continue
            var hook = hookBy[s.sessionId]
            var row = { s: s, depth: 0, project: board.effProject(s, byId, ps),
                        since: hook ? hook.updatedAt : s.startedAt }
            var k = board.kindOf(s)
            if (k === "shell") { terms.push(row); continue }
            var par = s.parentSessionId ? byId[s.parentSessionId] : null
            if ((k === "subagent" || s.nativeRole === "subagent") && par && board.isLive(par)) {
                row.depth = 1
                if (!kids[par.sessionId]) kids[par.sessionId] = []
                kids[par.sessionId].push(row)
            } else tops.push(row)
        }
        tops.sort(board.byStart); terms.sort(board.byStart)
        var agents = []
        for (i = 0; i < tops.length; i++) {
            agents.push(tops[i])
            var ch = kids[tops[i].s.sessionId] || []
            ch.sort(board.byStart)
            for (var c = 0; c < ch.length; c++) agents.push(ch[c])
        }
        var working = 0, awaiting = 0
        for (i = 0; i < agents.length; i++) {
            if (agents[i].s.state === "working") working++
            if (agents[i].s.state === "awaiting") awaiting++
        }
        // S1 bindings: graph.json's workspaces[].project (absent until S1)
        var jacksOf = {}
        var gw = board.graphWorkspaces || []
        for (i = 0; i < gw.length; i++) {
            var w = gw[i]
            if (!w || !w.project) continue
            if (!jacksOf[w.project]) jacksOf[w.project] = []
            jacksOf[w.project].push(w.workspace)
        }
        var projs = []
        for (i = 0; i < ps.length; i++) {
            var name = ps[i].name || ""
            var na = 0, nt = 0
            for (var a = 0; a < agents.length; a++) if (agents[a].project === name) na++
            for (var t = 0; t < terms.length; t++) if (terms[t].project === name) nt++
            projs.push({ name: name, path: ps[i].path || "", jacks: jacksOf[name] || [],
                         agents: na, terminals: nt })
        }
        return { agents: agents, terminals: terms, projects: projs,
                 working: working, awaiting: awaiting, byId: byId }
    }

    // the one amber lamp: the traced session, when the host names one
    readonly property string hotId: (board.shared && board.shared.tracedSessionId) ? ("" + board.shared.tracedSessionId) : ""

    // herald: summonses first (each waits on you), then newest first — the
    // ledger's own order is newest LAST (CONTRACTS §4 herald.json)
    readonly property var heraldRows: {
        var sum = [], rest = [], r = board.heraldRecs || []
        for (var i = r.length - 1; i >= 0; i--) {
            if (!r[i] || r[i].id === undefined) continue
            if (r[i].kind === "summons") sum.push(r[i]); else rest.push(r[i])
        }
        return sum.concat(rest)
    }
    readonly property int summonsCount: {
        var n = 0
        for (var i = 0; i < board.heraldRows.length; i++) if (board.heraldRows[i].kind === "summons") n++
        return n
    }

    // mail threads across every board answer (OVERVIEW MAIL), newest first
    readonly property var mailThreads: {
        var threads = {}, order = []
        var bs = board.boards || {}
        for (var p in bs) {
            var items = (bs[p] && bs[p].items) ? bs[p].items : []
            for (var i = 0; i < items.length; i++) {
                var it = items[i]
                if (!it || it.source !== "mail" || it.kind !== "letter") continue
                var key = it.thread || it.msgid || ("" + i)
                var th = threads[key]
                if (!th) { th = threads[key] = { key: key, mailbox: it.to || "", subject: it.subject || "",
                                                 from: it.from || "", at: it.at || "", count: 0 }; order.push(key) }
                th.count++
                if ((it.at || "") >= th.at) { th.at = it.at || ""; th.from = it.from || ""; if (it.subject) th.subject = it.subject }
            }
        }
        var out = order.map(function (k) { return threads[k] })
        out.sort(function (a, b) { return a.at < b.at ? 1 : (a.at > b.at ? -1 : 0) })
        return out
    }

    // ══ TABS ══════════════════════════════════════════════════════════════
    readonly property var tabs: {
        var t = ["overview"]
        var ps = board.projects || []
        for (var i = 0; i < ps.length; i++) if (ps[i] && ps[i].name) t.push("project:" + ps[i].name)
        t.push("sys"); t.push("notif")
        return t
    }
    function tabLabel(t) {
        if (t.indexOf("project:") === 0) return t.substring(8)
        return t.toUpperCase()
    }
    function openTab(t) {
        t = "" + (t || "overview")
        // a project tab is taken on trust: projects.json may not be read yet,
        // and onTabsChanged below drops it once the list says it is gone
        if (t.indexOf("project:") !== 0 && t !== "overview" && t !== "sys" && t !== "notif") t = "overview"
        if (t.indexOf("project:") === 0 && board.projects.length > 0 && board.tabs.indexOf(t) < 0) t = "overview"
        board.tab = t
    }
    function cycleTab(step) {
        var i = board.tabs.indexOf(board.tab)
        if (i < 0) i = 0
        var n = board.tabs.length
        board.tab = board.tabs[(i + step + n) % n]
    }
    // a project tab whose project was removed falls back
    onTabsChanged: if (board.tabs.indexOf(board.tab) < 0 && board.projects.length > 0) board.tab = "overview"

    // ══ ACTIONS (the only doors: the bridge) ══════════════════════════════
    function focus(row) {
        if (!row || !row.s || !board.bridge) return
        var s = row.s
        var id = (board.kindOf(s) === "subagent" && s.parentSessionId) ? s.parentSessionId : s.sessionId
        if (id) board.bridge.focusSession(id)
    }
    function heraldVerdict(sessionId, word) {
        if (board.bridge) board.bridge.sendCommand({ cmd: "heraldverdict", id: "" + sessionId, verdict: word })
    }
    function heraldDismiss(id) {
        if (board.bridge) board.bridge.sendCommand({ cmd: "heralddismiss", id: "" + id })
    }

    // ══ KEYS ══════════════════════════════════════════════════════════════
    focus: true
    Keys.onPressed: function (e) {
        if (e.key === Qt.Key_Escape) {
            if (view.item && view.item.cancel && view.item.cancel()) { e.accepted = true; return }
            board.closeRequested(); e.accepted = true; return
        }
        if (view.item && view.item.handleKey && view.item.handleKey(e)) { e.accepted = true; return }
        if (e.key === Qt.Key_Right || e.key === Qt.Key_L || e.key === Qt.Key_Tab) { board.cycleTab(1); e.accepted = true }
        else if (e.key === Qt.Key_Left || e.key === Qt.Key_H || e.key === Qt.Key_Backtab) { board.cycleTab(-1); e.accepted = true }
    }

    // ══ THE PANE ══════════════════════════════════════════════════════════
    // half a line of air above the pane: the title and stat ride the top
    // rule's line at y 0, and the window's top edge (under the bar) cut them
    // in half live (calendar.qml's topInset precedent)
    readonly property int topInset: Math.round(kit.cellH / 2)
    Use {
        id: frame
        anchors.fill: parent
        anchors.topMargin: board.topInset
        kit: board.kit; helper: "Pane"
        props: ({
            title: "board", glow: "bloom",
            open: Qt.binding(() => board.open),
            focused: Qt.binding(() => board.activeFocus),
            stat: Qt.binding(() => board.model.agents.length + board.model.terminals.length + " live"),
            content: frameBody
        })
    }

    Component {
        id: frameBody
        Item {
            width: parent ? parent.width : 0
            height: parent && parent.parent ? parent.parent.height : 0

            // ── the tab strip ─────────────────────────────────────────────
            Flickable {
                id: tabFlick
                width: parent.width; height: board.kit.cellH
                contentWidth: tabRow.implicitWidth
                interactive: contentWidth > width
                boundsBehavior: Flickable.StopAtBounds
                clip: true
                Row {
                    id: tabRow
                    Repeater {
                        model: board.tabs
                        Row {
                            id: tabCell
                            required property var modelData
                            required property int index
                            readonly property bool current: modelData === board.tab
                            readonly property bool isProject: modelData.indexOf("project:") === 0
                            Text {
                                visible: tabCell.index > 0
                                text: " │ "
                                color: board.kit.dim; font: board.kit.font; textFormat: Text.PlainText
                            }
                            Item {
                                width: label.implicitWidth + badge.implicitWidth; height: board.kit.cellH
                                Rectangle {
                                    anchors.fill: parent
                                    anchors.leftMargin: -Math.round(board.kit.cellW / 2)
                                    anchors.rightMargin: -Math.round(board.kit.cellW / 2)
                                    visible: tabCell.current
                                    color: board.kit.select
                                }
                                Text {
                                    id: label
                                    text: board.tabLabel(tabCell.modelData)
                                    color: tabCell.current ? board.kit.match
                                         : tabCell.isProject ? board.kit.path : board.kit.mid
                                    font: tabCell.current ? board.kit.titleFont : board.kit.font
                                    textFormat: Text.PlainText
                                    style: Text.Outline; styleColor: board.kit.withA(color, 0.18)
                                }
                                Text {
                                    id: badge
                                    x: label.implicitWidth
                                    visible: tabCell.modelData === "notif" && board.summonsCount > 0
                                    text: visible ? " !" + board.summonsCount : ""
                                    color: board.kit.urgent; font: board.kit.font; textFormat: Text.PlainText
                                }
                                MouseArea {
                                    anchors.fill: parent
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: { board.tab = tabCell.modelData; board.forceActiveFocus() }
                                }
                            }
                        }
                    }
                }
            }
            // the rule under the tabs
            Rectangle {
                y: Math.round(board.kit.cellH * 1.5); width: parent.width; height: 1
                color: board.kit.dim
            }

            // ── the view ──────────────────────────────────────────────────
            Loader {
                id: viewLoader
                y: board.kit.cellH * 2
                width: parent.width
                height: Math.max(0, parent.height - y)
                Component.onCompleted: view.loader = viewLoader
            }
        }
    }

    // the view Loader is created inside the pane's content Component; this
    // object holds the handle and (re)loads it by URL when the tab changes
    QtObject {
        id: view
        property var loader: null
        readonly property var item: loader ? loader.item : null
        function reload() {
            if (!loader) return
            var t = board.tab, helper = "BoardOverview", props = {}
            if (t === "sys") helper = "BoardSys"
            else if (t === "notif") helper = "BoardNotif"
            else if (t.indexOf("project:") === 0) { helper = "BoardProject"; props.project = t.substring(8) }
            props.kit = Qt.binding(() => board.kit)
            props.board = board
            loader.setSource(board.kit.helper(helper), props)
        }
        onLoaderChanged: reload()
    }
    onTabChanged: view.reload()
    onOpenChanged: if (board.open) board.nowMs = Date.now()
}
