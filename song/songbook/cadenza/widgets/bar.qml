// bar.qml — cadenza's "bar" slot: the switchboard line (intent §3.1, §3.2).
//
//   [⏻] [1] [2]aoide [3]aoide [5] [7]melete [9]mneme   title…   AGT 6/10 CPU 23% $ 4.20 !3 │ VOL 62% BT off NET wifi │ TRAY 2 RICE stg │ 14:02:31
//        └───┴────┴──────┘ ┆                                                  (ties hang in a 6px band under the jacks)
//   ─────────────────────────────────────────────────────────────────────────── the trunk
//
// WidgetSlot-hosted, root Item. The facet's PanelWindow reads `implicitHeight`
// (28) back for its height and exclusiveZone; width follows the host.
// Extras, all optional so the canvas (powermenu: null, dock: null) and a bare
// load never error: `shared`, `powermenu`, `dock`, `stagingEngine`.
//
// ── What it reads (rule 7: it paints, it holds no fact of its own) ─────────
//   Hyprland          workspaces (jack number, occupied, focused, urgent), active title
//   state/stage/sessions.json + hooks.json   per-jack sessions, AGT, urgent jacks
//   state/stage/graph.json  `workspaces[].project` (jack labels), `ties` (tie
//                           lines), `activeAt` (lamps) — core-seams §E; absent
//                           today, so no label, no tie, no lamp is drawn
//   state/usage/now.json    `by:"workspace"` rows → the jack insight pane —
//                           core-seams §C; absent today → "no usage data"
//   state/stage/herald.json `!n`
//   livery.usagePath        `$` (state/usage.json local.today.costUsd)
//   livery.riceMode         `RICE`
//   /proc/stat, /proc/meminfo, /proc/net/route   CPU, mem total, NET kind
//                           (kernel files, sonata meters/bar precedent)
//   Pipewire · Bluetooth · Networking · UPower · SystemTray  (sonata's services)
//
// ── What it does ───────────────────────────────────────────────────────────
//   [⏻] → powermenu.toggle()          jack click → workspace.activate()
//   AGT / CPU / $ / !n → dock.openTab("overview"|"sys"|"sys"|"notif"),
//        falling back to dock.toggle() when the dock has no openTab
//   RICE → bridge.toggleRiceMode()     session row → bridge.focusSession(id)
//   VOL/BT/NET/BAT/TRAY/clock → their own pane (one open at a time, click
//        the cell again to close); jack hover → the jack insight pane.
//   Service writes are sonata's: sink/source volume + mute + default,
//   adapter power + device connect, wifi on/off + connect/disconnect/forget,
//   tray activate/menu, `pavucontrol`, and `aoide spawn --windowed -- btop`.
//
// ── Panes ──────────────────────────────────────────────────────────────────
// Every pane is the kit's Pane (by URL), hosted in ONE PopupWindow (an
// xdg_popup of the bar) hung under its cell. `inlinePanes` (BarPreview.qml
// only) draws the same pane component inside this item instead, under the
// bar, because the canvas grabs an item, not a popup window.
//
// ── Ties and lamps (paint of core's `ties` + `activeAt`) ───────────────────
// Lanes are allocated in jack-INDEX space (interval colouring, first fit, by
// start), so geometry never feeds back into allocation. A project tie set
// touching 3+ jacks is one bus lane; ≤3 lanes; what does not fit is a `+n`
// badge (cyan) on its left jack. project = solid, spawned = dotted, both `mid`
// (a step above the dim trunk they hang over, so the wiring reads as data).
// Lamp: a 12px amber dash, 600ms linear, along the lane (tie) or up from the
// trunk into the socket (jack), only when `activeAt` ADVANCES between two
// reads; one in flight per line, later advances coalesce into it.
import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Hyprland
import Quickshell.Bluetooth
import Quickshell.Services.Pipewire
import Quickshell.Services.UPower
import Quickshell.Services.SystemTray
import Quickshell.Networking
// the facet's WidgetSlot, for the embedded calendar (sonata's bar does the same)
import "../.."
import "Kit.js" as Kit

Item {
    id: root

    required property var livery
    required property var bridge
    property var shared: null
    property var powermenu: null
    property var dock: null
    property var stagingEngine: null

    // ── preview knobs (BarPreview.qml sets these; the live path never does) ─
    property bool inlinePanes: false
    property int lampMs: 600

    // ── the kit ──────────────────────────────────────────────────────────────
    FontMetrics { id: fm; font: Kit.bodyFont(13) }
    readonly property var kit: Kit.make(root.livery, fm)

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

    // ── geometry: one text row, the tie band, the trunk ──────────────────────
    readonly property int barH: 28
    readonly property int socketY: 19          // a jack's socket rule
    readonly property int bandTop: 20          // the 6px tie band: 20..25
    readonly property int trunkY: barH - 1
    function laneY(l) { return bandTop + 1 + 2 * l }   // 21, 23, 25

    implicitHeight: barH
    implicitWidth: kit.cells(138)
    width: parent ? parent.width : implicitWidth

    readonly property string aoideRoot:
        Quickshell.env("AOIDE_ROOT") || (Quickshell.env("HOME") + "/.aoide")

    // ════════════════════════════════════════════════════════════════════════
    // DATA — stage files
    // ════════════════════════════════════════════════════════════════════════
    property var sessionRows: []
    property var hookPhase: ({})
    property var graphDoc: null
    property var nowDoc: null
    property var usageDoc: null
    property var heraldRows: []

    function parseJson(fv) {
        try { return JSON.parse(fv.text()) } catch (e) { return null }
    }

    FileView {
        id: sessionsFile
        path: root.aoideRoot + "/state/stage/sessions.json"
        watchChanges: true
        printErrors: false
        onFileChanged: reload()
        onTextChanged: {
            var d = root.parseJson(sessionsFile)
            if (d) root.sessionRows = Array.isArray(d.sessions) ? d.sessions : []
        }
        onLoadFailed: root.sessionRows = []
    }
    FileView {
        id: hooksFile
        path: root.aoideRoot + "/state/stage/hooks.json"
        watchChanges: true
        printErrors: false
        onFileChanged: reload()
        onTextChanged: {
            var d = root.parseJson(hooksFile)
            if (!d) return
            var m = {}
            var a = Array.isArray(d.hooks) ? d.hooks : []
            for (var i = 0; i < a.length; i++)
                if (a[i] && a[i].sessionId) m[a[i].sessionId] = "" + (a[i].phase || "")
            root.hookPhase = m
        }
        onLoadFailed: root.hookPhase = ({})
    }
    FileView {
        id: heraldFile
        path: root.aoideRoot + "/state/stage/herald.json"
        watchChanges: true
        printErrors: false
        onFileChanged: reload()
        onTextChanged: {
            var d = root.parseJson(heraldFile)
            if (d) root.heraldRows = Array.isArray(d.notifications) ? d.notifications : []
        }
        onLoadFailed: root.heraldRows = []
    }
    // graph.json: the ties block rides only when core writes it (core-seams §E)
    property bool graphOk: false
    FileView {
        id: graphFile
        path: root.aoideRoot + "/state/stage/graph.json"
        watchChanges: true
        printErrors: false
        onFileChanged: reload()
        onTextChanged: {
            var d = root.parseJson(graphFile)
            if (!d) return
            root.graphOk = true
            root.graphDoc = d
            root.detectLamps(d)
        }
        onLoadFailed: { root.graphOk = false; root.graphDoc = null }
    }
    // state/usage/now.json — the S5/S6 store (core-seams §C); absent today
    property bool nowOk: false
    FileView {
        id: nowFile
        path: root.aoideRoot + "/state/usage/now.json"
        watchChanges: true
        printErrors: false
        onFileChanged: reload()
        onTextChanged: {
            var d = root.parseJson(nowFile)
            if (!d) return
            root.nowOk = true
            root.nowDoc = d
        }
        onLoadFailed: { root.nowOk = false; root.nowDoc = null }
    }
    property bool usageOk: false
    FileView {
        id: usageFile
        path: root.livery.usagePath
        watchChanges: true
        printErrors: false
        onFileChanged: reload()
        onTextChanged: {
            var d = root.parseJson(usageFile)
            if (!d) return
            root.usageOk = true
            root.usageDoc = d
        }
        onLoadFailed: { root.usageOk = false; root.usageDoc = null }
    }
    // A file that did not exist at load has no watch to wake it; look again.
    Timer {
        interval: 5000; repeat: true; running: true
        onTriggered: {
            if (!root.graphOk) graphFile.reload()
            if (!root.nowOk) nowFile.reload()
            if (!root.usageOk) usageFile.reload()
        }
    }

    // ── merged session state (a live hook phase overrides the roster state) ─
    function normState(s) {
        var v = ("" + (s || "")).toLowerCase()
        if (v.indexOf("block") >= 0) return "awaiting"
        return v
    }
    readonly property var sessions: {
        var out = []
        var a = root.sessionRows
        for (var i = 0; i < a.length; i++) {
            var s = a[i]
            if (!s) continue
            var id = "" + (s.sessionId || s.id || "")
            var ph = root.hookPhase[id]
            out.push({
                id: id,
                state: root.normState(ph ? ph : s.state),
                ws: (s.workspace === null || s.workspace === undefined) ? -1 : s.workspace,
                kind: "" + (s.kind || ""),
                name: "" + (s.petname || s.agent || id.slice(0, 8)),
                agent: "" + (s.agent || "")
            })
        }
        return out
    }
    readonly property var agentStats: {
        var t = 0, w = 0, a = 0
        var s = root.sessions
        for (var i = 0; i < s.length; i++) {
            if (s[i].kind === "shell") continue
            t++
            if (s[i].state === "working") w++
            if (s[i].state === "awaiting") a++
        }
        return { total: t, working: w, awaiting: a }
    }
    readonly property var heraldStats: {
        var n = root.heraldRows.length, summons = false
        for (var i = 0; i < n; i++)
            if (root.heraldRows[i] && root.heraldRows[i].kind === "summons") summons = true
        return { count: n, summons: summons }
    }

    // ════════════════════════════════════════════════════════════════════════
    // THE SWITCHBOARD — jacks, routes, geometry
    // ════════════════════════════════════════════════════════════════════════
    readonly property var graphWs: {
        var m = {}
        var a = (root.graphDoc && Array.isArray(root.graphDoc.workspaces)) ? root.graphDoc.workspaces : []
        for (var i = 0; i < a.length; i++)
            if (a[i] && typeof a[i].workspace === "number") m[a[i].workspace] = a[i]
        return m
    }
    readonly property var graphTies:
        (root.graphDoc && Array.isArray(root.graphDoc.ties)) ? root.graphDoc.ties : []

    readonly property var hyprWs: {
        var m = {}
        var vs = (Hyprland.workspaces && Hyprland.workspaces.values) ? Hyprland.workspaces.values : []
        for (var i = 0; i < vs.length; i++)
            if (vs[i] && vs[i].id > 0) m[vs[i].id] = vs[i]
        return m
    }

    // The jacks: every live Hyprland workspace, plus any workspace core names
    // (a session's, a binding's, a tie end) — so a label or tie never dangles.
    readonly property var jacks: {
        var ids = {}
        var k
        for (k in root.hyprWs) ids[k] = true
        for (k in root.graphWs) ids[k] = true
        var s = root.sessions
        for (var i = 0; i < s.length; i++) if (s[i].ws > 0) ids[s[i].ws] = true
        var t = root.graphTies
        for (var j = 0; j < t.length; j++) {
            if (t[j] && t[j].from > 0) ids[t[j].from] = true
            if (t[j] && t[j].to > 0) ids[t[j].to] = true
        }
        var list = Object.keys(ids).map(function (x) { return parseInt(x) })
        list.sort(function (a, b) { return a - b })
        var out = []
        for (var n = 0; n < list.length; n++) {
            var id = list[n]
            var w = root.hyprWs[id] || null
            var g = root.graphWs[id] || null
            var sess = [], urgent = false
            for (var q = 0; q < s.length; q++) {
                if (s[q].ws !== id) continue
                sess.push(s[q])
                if (s[q].state === "awaiting") urgent = true
            }
            var tops = (w && w.toplevels && w.toplevels.values) ? w.toplevels.values.length : 0
            out.push({
                id: id,
                ws: w,
                focused: !!(w && w.focused),
                urgent: urgent || !!(w && w.urgent),
                occupied: tops > 0 || sess.length > 0 || !!(g && g.live > 0),
                project: (g && typeof g.project === "string") ? g.project : "",
                live: g ? (g.live || 0) : sess.length,
                sessions: sess
            })
        }
        return out
    }

    // routes in jack-index space: {key, kind, members[idx], a, b, from, to, lane}
    readonly property var routing: {
        var idx = {}
        var J = root.jacks
        for (var i = 0; i < J.length; i++) idx[J[i].id] = i
        var byKey = {}, order = []
        var T = root.graphTies
        for (var t = 0; t < T.length; t++) {
            var e = T[t]
            if (!e || !(e.from in idx) || !(e.to in idx) || e.from === e.to) continue
            var key, kind = e.kind === "spawned" ? "spawned" : "project"
            key = kind === "project" ? "p:" + (e.project || "?") : "s:" + e.from + ">" + e.to
            var r = byKey[key]
            if (!r) {
                r = byKey[key] = { key: key, kind: kind, set: {}, from: idx[e.from], to: idx[e.to] }
                order.push(key)
            }
            r.set[idx[e.from]] = true
            r.set[idx[e.to]] = true
        }
        var routes = []
        for (var o = 0; o < order.length; o++) {
            var rr = byKey[order[o]]
            var m = Object.keys(rr.set).map(function (x) { return parseInt(x) })
            m.sort(function (a, b) { return a - b })
            rr.members = m
            rr.a = m[0]
            rr.b = m[m.length - 1]
            rr.lane = -1
            routes.push(rr)
        }
        routes.sort(function (p, q) { return p.a !== q.a ? p.a - q.a : p.b - q.b })
        var laneEnd = [-1, -1, -1]
        var badges = {}
        for (var r2 = 0; r2 < routes.length; r2++) {
            var R = routes[r2]
            for (var l = 0; l < 3; l++) {
                if (laneEnd[l] < R.a) { R.lane = l; laneEnd[l] = R.b; break }
            }
            if (R.lane < 0) {
                var jid = J[R.a].id
                badges[jid] = (badges[jid] || 0) + 1
            }
        }
        return { routes: routes, badges: badges }
    }

    // jack geometry, in whole cells: socket "[n]", project label, "+n" badge
    readonly property var jackModel: {
        var J = root.jacks, B = root.routing.badges
        var out = [], xc = 0
        for (var i = 0; i < J.length; i++) {
            var j = J[i]
            var sock = "[" + j.id + "]"
            var badge = B[j.id] ? "+" + B[j.id] : ""
            var n = sock.length + j.project.length + badge.length
            out.push({
                id: j.id, ws: j.ws, focused: j.focused, urgent: j.urgent,
                occupied: j.occupied, project: j.project, live: j.live,
                sessions: j.sessions, sock: sock, badge: badge,
                x: root.kit.cells(xc),
                w: root.kit.cells(n),
                sockW: root.kit.cells(sock.length),
                cx: Math.round(root.kit.cells(xc) + root.kit.cells(sock.length) / 2)
            })
            xc += n + 1
        }
        return out
    }
    readonly property int boardW: jackModel.length > 0
        ? jackModel[jackModel.length - 1].x + jackModel[jackModel.length - 1].w : 0

    function jackIndex(id) {
        for (var i = 0; i < root.jackModel.length; i++)
            if (root.jackModel[i].id === id) return i
        return -1
    }
    function jackColor(j) {
        if (j.urgent) return root.kit.urgent
        if (j.focused) return root.kit.title
        if (j.occupied) return root.kit.ink
        return root.kit.dim
    }
    // a lane's drop sits a little off the socket centre so stacked lanes read apart
    function dropX(idx, lane) { return root.jackModel[idx].cx + (lane - 1) * 2 }

    // ── lamps ────────────────────────────────────────────────────────────────
    property var _seen: ({})
    property bool _seenInit: false
    property var _lamps: ({})
    function _t(s) { var v = Date.parse(s || ""); return isNaN(v) ? 0 : v }
    function detectLamps(doc) {
        var next = {}
        var ws = Array.isArray(doc.workspaces) ? doc.workspaces : []
        for (var i = 0; i < ws.length; i++)
            if (ws[i] && ws[i].activeAt) next["w:" + ws[i].workspace] = root._t(ws[i].activeAt)
        var ties = Array.isArray(doc.ties) ? doc.ties : []
        for (var j = 0; j < ties.length; j++) {
            var e = ties[j]
            if (!e || !e.activeAt) continue
            var key = e.kind === "spawned" ? "s:" + e.from + ">" + e.to : "p:" + (e.project || "?")
            next[key] = Math.max(next[key] || 0, root._t(e.activeAt))
        }
        var prev = root._seen
        var fire = []
        if (root._seenInit)
            for (var k in next)
                if (prev[k] !== undefined && next[k] > prev[k]) fire.push(k)
        root._seen = next
        root._seenInit = true
        // after the new doc has re-laid the switchboard
        if (fire.length > 0) Qt.callLater(function () {
            for (var f = 0; f < fire.length; f++) root.fireLamp(fire[f])
        })
    }
    function fireLamp(key) {
        if (root._lamps[key]) return                    // coalesce into the one in flight
        var p = null
        if (key.indexOf("w:") === 0) {
            var i = root.jackIndex(parseInt(key.slice(2)))
            if (i < 0) return
            var cx = root.jackModel[i].cx
            p = { x0: cx - 6, x1: cx - 6, y0: root.trunkY, y1: root.socketY }
        } else {
            var R = null, rs = root.routing.routes
            for (var r = 0; r < rs.length; r++) if (rs[r].key === key) R = rs[r]
            if (!R || R.lane < 0) return                // a badge carries no line to run
            var a = R.a, b = R.b
            if (R.kind === "spawned") { a = R.from; b = R.to }
            var xa = root.dropX(a, R.lane), xb = root.dropX(b, R.lane)
            var y = root.laneY(R.lane)
            p = { x0: xa - 6, x1: xb - 6, y0: y, y1: y }
        }
        p.key = key
        p.dur = root.lampMs
        var o = lampComp.createObject(board, p)
        if (o) root._lamps[key] = o
    }
    function lampDone(key, o) {
        delete root._lamps[key]
        o.destroy()
    }
    Component {
        id: lampComp
        Rectangle {
            id: lamp
            property string key: ""
            property real x0: 0
            property real x1: 0
            property real y0: 0
            property real y1: 0
            property int dur: 600
            x: x0; y: y0
            width: 12; height: 1
            color: root.kit.hot
            ParallelAnimation {
                id: run
                NumberAnimation { target: lamp; property: "x"; from: lamp.x0; to: lamp.x1; duration: lamp.dur; easing.type: Easing.Linear }
                NumberAnimation { target: lamp; property: "y"; from: lamp.y0; to: lamp.y1; duration: lamp.dur; easing.type: Easing.Linear }
                onFinished: root.lampDone(lamp.key, lamp)
            }
            Component.onCompleted: run.start()
        }
    }

    // ════════════════════════════════════════════════════════════════════════
    // DATA — machine (kernel files) and services
    // ════════════════════════════════════════════════════════════════════════
    property real cpuPct: -1
    property var _cpuPrev: null
    property real memTotal: 0
    FileView { id: statFile; path: "/proc/stat"; blockLoading: true; printErrors: false }
    FileView { id: memFile; path: "/proc/meminfo"; blockLoading: true; printErrors: false }
    function sampleCpu() {
        statFile.reload()
        var line = ("" + statFile.text()).split("\n")[0] || ""
        var p = line.trim().split(/\s+/).slice(1).map(Number)
        if (p.length < 4) return
        var idle = p[3] + (p[4] || 0), total = 0
        for (var i = 0; i < p.length; i++) total += p[i] || 0
        if (root._cpuPrev) {
            var dt = total - root._cpuPrev.total
            if (dt > 0) root.cpuPct = Math.max(0, Math.min(100, 100 * (1 - (idle - root._cpuPrev.idle) / dt)))
        }
        root._cpuPrev = { idle: idle, total: total }
    }
    Timer { interval: 2000; repeat: true; running: true; onTriggered: root.sampleCpu() }

    // ── clock ───────────────────────────────────────────────────────────────
    property var now: new Date()
    Timer { interval: 1000; repeat: true; running: true; onTriggered: root.now = new Date() }
    function two(n) { return n < 10 ? "0" + n : "" + n }
    readonly property string clockText:
        two(now.getHours()) + ":" + two(now.getMinutes()) + ":" + two(now.getSeconds())

    // ── Pipewire (sonata's seam) ────────────────────────────────────────────
    PwObjectTracker {
        objects: {
            var o = []
            if (Pipewire.defaultAudioSink) o.push(Pipewire.defaultAudioSink)
            if (Pipewire.defaultAudioSource) o.push(Pipewire.defaultAudioSource)
            return o
        }
    }
    readonly property var sinkAudio: (Pipewire.ready && Pipewire.defaultAudioSink
                                      && Pipewire.defaultAudioSink.audio) ? Pipewire.defaultAudioSink.audio : null
    readonly property var srcAudio: (Pipewire.ready && Pipewire.defaultAudioSource
                                     && Pipewire.defaultAudioSource.audio) ? Pipewire.defaultAudioSource.audio : null
    readonly property bool volMuted: sinkAudio ? sinkAudio.muted : false
    readonly property int volPct: sinkAudio ? Math.round(sinkAudio.volume * 100) : 0
    readonly property bool micMuted: srcAudio ? srcAudio.muted : false
    readonly property int micPct: srcAudio ? Math.round(srcAudio.volume * 100) : 0
    function setVol(a, v) { if (a) a.volume = Math.max(0, Math.min(1, v)) }
    function nudgeVol(a, d) { if (a) root.setVol(a, a.volume + d / 100) }
    function toggleMute(a) { if (a) a.muted = !a.muted }

    property int pwEpoch: 0
    Connections {
        target: Pipewire.nodes
        function onObjectInsertedPost(obj, index) { root.pwEpoch++ }
        function onObjectRemovedPost(obj, index) { root.pwEpoch++ }
    }
    function nodeLabel(n) {
        var s = "" + (n.nickname || n.description || n.name || "")
        return s.length > 0 ? s : "node " + n.id
    }
    function nodeGloss(n) {
        var s = "" + (n.name || ""), dot = s.lastIndexOf(".")
        return dot >= 0 ? s.substring(dot + 1) : s
    }
    function pwRoster(wantSink) {
        var epoch = root.pwEpoch
        var out = []
        if (!Pipewire.ready) return out
        var vals = (Pipewire.nodes && Pipewire.nodes.values) ? Pipewire.nodes.values : []
        var cur = wantSink ? Pipewire.defaultAudioSink : Pipewire.defaultAudioSource
        for (var i = 0; i < vals.length; i++) {
            var n = vals[i]
            if (!n || n.isStream) continue
            if (!(n.type & PwNodeType.Audio)) continue
            if (n.isSink !== wantSink) continue
            out.push({ key: "" + n.id, name: root.nodeLabel(n), gloss: root.nodeGloss(n),
                       current: !!cur && cur.id === n.id })
        }
        return out
    }
    readonly property var sinkRoster: pwRoster(true)
    readonly property var sourceRoster: pwRoster(false)
    function pwNode(key) {
        var vals = (Pipewire.nodes && Pipewire.nodes.values) ? Pipewire.nodes.values : []
        for (var i = 0; i < vals.length; i++)
            if (vals[i] && ("" + vals[i].id) === ("" + key)) return vals[i]
        return null
    }
    function pickSink(key) { var n = root.pwNode(key); if (n) Pipewire.preferredDefaultAudioSink = n }
    function pickSource(key) { var n = root.pwNode(key); if (n) Pipewire.preferredDefaultAudioSource = n }
    function openMixer() { Quickshell.execDetached(["pavucontrol"]) }

    // ── Bluetooth (sonata's seam) ───────────────────────────────────────────
    readonly property var btAdapter: Bluetooth.defaultAdapter
    readonly property bool btOn: !!btAdapter && btAdapter.enabled
    property var btDev: null
    property int btEpoch: 0
    function rescanBt() {
        var vals = (Bluetooth.devices && Bluetooth.devices.values) ? Bluetooth.devices.values : []
        var found = null
        for (var i = 0; i < vals.length; i++) if (vals[i] && vals[i].connected) { found = vals[i]; break }
        root.btDev = found
        root.btEpoch++
    }
    readonly property var btRoster: {
        var epoch = root.btEpoch
        var out = []
        var vals = (Bluetooth.devices && Bluetooth.devices.values) ? Bluetooth.devices.values : []
        for (var i = 0; i < vals.length; i++) {
            var d = vals[i]
            if (!d) continue
            out.push({ key: "" + d.address, name: "" + (d.deviceName || d.name || d.address),
                       gloss: d.connected ? "connected" : (d.paired ? "paired" : "seen"),
                       current: d.connected === true })
        }
        return out
    }
    function pickBt(key) {
        var vals = (Bluetooth.devices && Bluetooth.devices.values) ? Bluetooth.devices.values : []
        for (var i = 0; i < vals.length; i++) {
            var d = vals[i]
            if (!d || ("" + d.address) !== ("" + key)) continue
            if (d.connected) d.disconnect(); else d.connect()
            return
        }
    }
    function toggleBtPower() { if (root.btAdapter) root.btAdapter.enabled = !root.btAdapter.enabled }
    Connections {
        target: Bluetooth
        function onDefaultAdapterChanged() { root.rescanBt() }
    }
    Connections {
        target: Bluetooth.devices
        function onObjectInsertedPost(obj, index) { root.rescanBt() }
        function onObjectRemovedPost(obj, index) { root.rescanBt() }
    }
    Repeater {                                   // the model is silent on a connect
        model: Bluetooth.devices
        delegate: Item {
            required property var modelData
            width: 0; height: 0; visible: false
            readonly property bool conn: modelData ? modelData.connected : false
            onConnChanged: root.rescanBt()
        }
    }

    // ── Network: kind from /proc/net/route, the pane from Networking ────────
    property string netKind: "down"
    function parseRoute(text) {
        var lines = ("" + (text || "")).split("\n")
        for (var i = 1; i < lines.length; i++) {
            var p = lines[i].trim().split(/\s+/)
            if (p.length >= 2 && p[1] === "00000000")
                return p[0].toLowerCase().indexOf("wl") === 0 ? "wifi" : "eth"
        }
        return "down"
    }
    FileView {
        id: routeFile
        path: "/proc/net/route"
        printErrors: false
        onTextChanged: root.netKind = root.parseRoute(routeFile.text())
    }
    Timer { interval: 5000; repeat: true; running: true; onTriggered: routeFile.reload() }

    property var wifiDev: null
    property var wiredDev: null
    function rescanNet() {
        var vals = (Networking.devices && Networking.devices.values) ? Networking.devices.values : []
        var w = null, e = null
        for (var i = 0; i < vals.length; i++) {
            var d = vals[i]
            if (!d) continue
            if (!w && d.type === DeviceType.Wifi) w = d
            if (!e && d.type === DeviceType.Wired) e = d
        }
        root.wifiDev = w
        root.wiredDev = e
    }
    Connections {
        target: Networking.devices
        function onObjectInsertedPost(obj, index) { root.rescanNet() }
        function onObjectRemovedPost(obj, index) { root.rescanNet() }
    }
    Binding {                                   // scan only while the pane is open
        target: root.wifiDev
        property: "scannerEnabled"
        value: root.openPane === "net"
        when: root.wifiDev !== null
    }
    property int netEpoch: 0
    Repeater {
        model: (root.openPane === "net" && root.wifiDev) ? root.wifiDev.networks : null
        delegate: Item {
            required property var modelData
            width: 0; height: 0; visible: false
            readonly property real sway: modelData ? modelData.signalStrength : 0
            readonly property bool conn: modelData ? modelData.connected : false
            readonly property bool kn: modelData ? modelData.known : false
            onSwayChanged: root.netEpoch++
            onConnChanged: root.netEpoch++
            onKnChanged: root.netEpoch++
            Component.onCompleted: root.netEpoch++
        }
    }
    function sigPct(s) { var v = s <= 1.0 ? s * 100 : s; return Math.max(0, Math.min(100, Math.round(v))) }
    function secured(s) { return s !== WifiSecurityType.Open && s !== WifiSecurityType.Owe }
    readonly property var wifiList: {
        var e = root.netEpoch
        var d = root.wifiDev
        var vals = (d && d.networks && d.networks.values) ? d.networks.values.slice() : []
        vals = vals.filter(function (n) { return n && ("" + n.name).length > 0 })
        vals.sort(function (a, b) {
            if (a.connected !== b.connected) return a.connected ? -1 : 1
            if (a.known !== b.known) return a.known ? -1 : 1
            return (b.signalStrength || 0) - (a.signalStrength || 0)
        })
        return vals.slice(0, 8)
    }
    function netWord() {
        var c = Networking.connectivity
        if (c === NetworkConnectivity.Full) return "full"
        if (c === NetworkConnectivity.Limited) return "limited"
        if (c === NetworkConnectivity.Portal) return "portal"
        if (c === NetworkConnectivity.None) return "none"
        return ""
    }

    // ── UPower ──────────────────────────────────────────────────────────────
    readonly property var battDev: UPower.displayDevice
    readonly property bool battAvail: !!battDev && battDev.isLaptopBattery && battDev.isPresent
    readonly property int battPct: battDev ? Math.round(battDev.percentage * 100) : 0
    readonly property bool battCharging: !!battDev && battDev.state === UPowerDeviceState.Charging
    readonly property bool battFull: !!battDev && battDev.state === UPowerDeviceState.FullyCharged
    function battTime() {
        if (!battDev) return ""
        var sec = battCharging ? battDev.timeToFull : battDev.timeToEmpty
        if (!sec || sec <= 0) return ""
        var min = Math.round(sec / 60)
        return Math.floor(min / 60) + "h" + two(min % 60)
    }

    // ── SystemTray ──────────────────────────────────────────────────────────
    property int trayCount: 0
    function recountTray() {
        var v = SystemTray.items ? SystemTray.items.values : null
        root.trayCount = v ? v.length : 0
        if (root.trayCount === 0 && root.openPane === "tray") root.closePane()
    }
    Connections {
        target: SystemTray.items
        function onObjectInsertedPost(obj, index) { root.recountTray() }
        function onObjectRemovedPost(obj, index) { root.recountTray() }
    }

    Component.onCompleted: {
        memFile.reload()
        var m = /MemTotal:\s+(\d+)/.exec("" + memFile.text())
        if (m) root.memTotal = parseInt(m[1]) * 1024
        root.sampleCpu()
        routeFile.reload()
        root.rescanBt()
        root.rescanNet()
        root.recountTray()
        for (var f of [sessionsFile, hooksFile, heraldFile, graphFile, nowFile, usageFile]) f.reload()
    }

    // ── formatting ──────────────────────────────────────────────────────────
    function fmtTok(n) {
        if (n === null || n === undefined || isNaN(n)) return "—"
        if (n >= 1e6) return (n / 1e6).toFixed(1) + "M"
        if (n >= 1e3) return (n / 1e3).toFixed(1) + "k"
        return "" + Math.round(n)
    }
    function fmtBytes(b) {
        if (b === null || b === undefined || isNaN(b)) return "—"
        if (b >= 1073741824) return (b / 1073741824).toFixed(1) + "G"
        return Math.round(b / 1048576) + "M"
    }
    readonly property var todayCost: {
        var u = root.usageDoc
        var v = (u && u.local && u.local.today) ? u.local.today.costUsd : null
        return (typeof v === "number") ? v : null
    }
    function modeWord(m) { return m === "staging" ? "stg" : (m === "draft" ? "drft" : "decl") }
    function modeColor(m) { return m === "staging" ? root.kit.title : (m === "draft" ? root.kit.path : root.kit.dim) }

    // ── actions ─────────────────────────────────────────────────────────────
    function openBoard(tab) {
        if (root.dock && root.dock.openTab) root.dock.openTab(tab)
        else if (root.dock && root.dock.toggle) root.dock.toggle()
    }
    function launchBtop() {
        Quickshell.execDetached(["aoide", "spawn", "--windowed", "--agent", "btop", "--", "btop"])
    }

    // ════════════════════════════════════════════════════════════════════════
    // PANE HOSTING — one pane open at a time, hung under its cell
    // ════════════════════════════════════════════════════════════════════════
    property string openPane: ""
    property Item paneCell: null
    property bool paneRight: true
    property real paneCellX: 0
    property real paneCellW: 0
    property bool popupGate: false
    property int insightId: -1          // the jack the insight pane reads
    property bool jackHovered: false
    property bool paneHovered: false

    function showPane(name, cell, right) {
        root.popupGate = false
        root.openPane = name
        root.paneCell = cell
        root.paneRight = right
        var p = cell.mapToItem(root, 0, 0)
        root.paneCellX = p.x
        root.paneCellW = cell.width
        // a fresh xdg_popup per open: the anchor is read when the popup maps
        Qt.callLater(function () { root.popupGate = true })
    }
    function togglePane(name, cell, right) {
        if (root.openPane === name) root.closePane()
        else root.showPane(name, cell, right)
    }
    function closePane() {
        root.popupGate = false
        root.openPane = ""
        root.paneCell = null
        root.paneHovered = false
    }
    function jackEnter(id) {
        root.jackHovered = true
        hoverGrace.stop()
        var i = root.jackIndex(id)
        if (i < 0) return
        if (root.openPane === "jack" && root.insightId === id) return
        if (root.openPane !== "" && root.openPane !== "jack") return   // a latched pane wins
        root.insightId = id
        jackAnchor.x = root.jackModel[i].x
        jackAnchor.width = root.jackModel[i].w
        root.showPane("jack", jackAnchor, false)
    }
    function jackLeave() { root.jackHovered = false; hoverGrace.restart() }
    Timer {
        id: hoverGrace
        interval: 220
        onTriggered: if (root.openPane === "jack" && !root.jackHovered && !root.paneHovered) root.closePane()
    }

    // BarPreview hooks: open a named pane / a jack's insight pane without a pointer
    function previewPane(name) {
        var c = ({ vol: volCell, bt: btCell, net: netCell, bat: batCell, tray: trayCell, clock: clockCell })[name]
        if (c) root.showPane(name, c, true)
    }
    function previewJack(id) { root.jackEnter(id); root.jackHovered = false }

    function paneComponent(name) {
        switch (name) {
        case "vol": return volPane
        case "bt": return btPane
        case "net": return netPane
        case "bat": return batPane
        case "tray": return trayPane
        case "clock": return clockPane
        case "jack": return jackPane
        }
        return null
    }
    readonly property Component paneComp: paneComponent(openPane)

    PopupWindow {
        id: popup
        visible: !root.inlinePanes && root.popupGate && root.openPane !== ""
                 && root.paneCell !== null && popLoader.item !== null
        anchor.item: root.paneCell
        anchor.edges: root.paneRight ? (Edges.Bottom | Edges.Right) : (Edges.Bottom | Edges.Left)
        anchor.gravity: root.paneRight ? (Edges.Bottom | Edges.Left) : (Edges.Bottom | Edges.Right)
        color: "transparent"
        implicitWidth: Math.max(1, popLoader.implicitWidth)
        implicitHeight: Math.max(1, popLoader.implicitHeight)
        Loader {
            id: popLoader
            active: !root.inlinePanes && root.popupGate
            sourceComponent: root.paneComp
            HoverHandler {
                onHoveredChanged: {
                    root.paneHovered = hovered
                    if (!hovered) hoverGrace.restart()
                }
            }
        }
    }
    // preview only: the same pane drawn under the bar, inside this item
    Loader {
        id: inlineLoader
        active: root.inlinePanes && root.openPane !== ""
        sourceComponent: root.paneComp
        y: root.barH
        x: {
            var w = item ? item.implicitWidth : 0
            var px = root.paneRight ? root.paneCellX + root.paneCellW - w : root.paneCellX
            return Math.max(0, Math.min(root.width - w, px))
        }
    }

    // ════════════════════════════════════════════════════════════════════════
    // PAINT — the line
    // ════════════════════════════════════════════════════════════════════════
    // one cell: `LABEL value`, label dim (accent while its pane is open)
    component Cell: Item {
        id: cell
        required property var kit
        property string label: ""
        property string value: ""
        property color valueColor: kit.ink
        property bool lit: false
        signal activated()
        signal wheeled(int delta)
        readonly property int gap: (label.length > 0 && value.length > 0) ? 1 : 0
        implicitWidth: kit.cells(label.length + gap + value.length)
        width: implicitWidth
        height: parent ? parent.height : 0
        Text {
            text: cell.label
            color: (cell.lit || cellMa.containsMouse) ? cell.kit.title : cell.kit.dim
            font: cell.kit.font
            textFormat: Text.PlainText
            style: Text.Outline
            styleColor: cell.kit.withA(color, 0.18)
        }
        Text {
            x: cell.kit.cells(cell.label.length + cell.gap)
            text: cell.value
            color: cell.valueColor
            font: cell.kit.font
            textFormat: Text.PlainText
            style: Text.Outline
            styleColor: cell.kit.withA(color, 0.18)
        }
        MouseArea {
            id: cellMa
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: cell.activated()
            onWheel: function (w) { cell.wheeled(w.angleDelta.y) }
        }
    }
    component Sep: Text {
        required property var kit
        text: "│"
        color: kit.dim
        font: kit.font
        textFormat: Text.PlainText
    }

    // the field
    Rectangle { anchors.fill: parent; color: root.kit.ground }
    // the trunk — every tie hangs over it; dark at rest
    Rectangle { x: 0; y: root.trunkY; width: root.width; height: 1; color: root.kit.dim }

    // ── [⏻] — the power key ────────────────────────────────────────────────
    Item {
        id: powerKey
        x: root.kit.cellW
        width: root.kit.cells(3)
        height: root.barH
        Text {
            text: "["; color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText
        }
        Text {
            x: root.kit.cellW
            text: "⏻"
            color: powerMa.containsMouse ? root.kit.title : root.kit.ink
            font: root.kit.font
            textFormat: Text.PlainText
            style: Text.Outline
            styleColor: root.kit.withA(color, 0.18)
        }
        Text {
            x: root.kit.cells(2)
            text: "]"; color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText
        }
        MouseArea {
            id: powerMa
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: if (root.powermenu && root.powermenu.toggle) root.powermenu.toggle()
        }
    }

    // ── the switchboard ─────────────────────────────────────────────────────
    Item {
        id: board
        x: powerKey.x + powerKey.width + root.kit.cellW
        height: root.barH
        width: Math.min(root.boardW, Math.max(0, right.x - x - root.kit.cells(2)))
        clip: root.boardW > width

        // tie lines: one Canvas, repainted only when the routes or the palette move
        Canvas {
            id: ties
            width: Math.max(1, root.boardW)
            height: root.barH
            readonly property var paintKey: [root.routing, root.jackModel, root.kit.mid]
            onPaintKeyChanged: requestPaint()
            Component.onCompleted: requestPaint()
            onPaint: {
                var ctx = getContext("2d")
                ctx.reset()
                ctx.clearRect(0, 0, width, height)
                ctx.fillStyle = "" + root.kit.mid
                var rs = root.routing.routes
                for (var r = 0; r < rs.length; r++) {
                    var R = rs[r]
                    if (R.lane < 0) continue
                    var y = root.laneY(R.lane)
                    var step = R.kind === "spawned" ? 2 : 1
                    var xs = R.members.map(function (m) { return root.dropX(m, R.lane) })
                    var x0 = Math.min.apply(null, xs), x1 = Math.max.apply(null, xs)
                    for (var x = x0; x <= x1; x += step) ctx.fillRect(x, y, 1, 1)
                    for (var k = 0; k < xs.length; k++)
                        for (var yy = root.bandTop; yy < y; yy += step) ctx.fillRect(xs[k], yy, 1, 1)
                }
            }
        }

        // the jacks
        Repeater {
            model: root.jackModel
            delegate: Item {
                id: jack
                required property var modelData
                readonly property color tone: root.jackColor(modelData)
                readonly property bool previewed: !!root.shared && root.shared.hoveredWorkspace === modelData.id
                x: modelData.x
                width: modelData.w
                height: root.barH

                // the board's terminal-row hover names this jack
                Rectangle {
                    visible: jack.previewed
                    width: jack.modelData.sockW; height: root.kit.cellH
                    color: root.kit.select
                }
                Text {
                    text: jack.modelData.sock
                    color: jack.tone
                    font.family: root.kit.font.family
                    font.pixelSize: root.kit.font.pixelSize
                    font.bold: jack.modelData.focused
                    textFormat: Text.PlainText
                    style: Text.Outline
                    styleColor: root.kit.withA(color, 0.18)
                }
                Text {
                    x: jack.modelData.sockW
                    visible: jack.modelData.project.length > 0
                    text: jack.modelData.project
                    color: root.kit.path
                    font: root.kit.font
                    textFormat: Text.PlainText
                }
                Text {
                    x: root.kit.cells(jack.modelData.sock.length + jack.modelData.project.length)
                    visible: jack.modelData.badge.length > 0
                    text: jack.modelData.badge
                    color: root.kit.number
                    font: root.kit.font
                    textFormat: Text.PlainText
                }
                // the socket rule: an occupied jack is a solid rule, an empty one none
                Rectangle {
                    visible: jack.modelData.occupied || jack.modelData.focused
                    x: 1; y: root.socketY
                    width: jack.modelData.sockW - 2; height: 1
                    color: jack.tone
                }
                MouseArea {
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: jack.modelData.ws ? Qt.PointingHandCursor : Qt.ArrowCursor
                    onEntered: root.jackEnter(jack.modelData.id)
                    onExited: root.jackLeave()
                    onClicked: if (jack.modelData.ws && jack.modelData.ws.activate) jack.modelData.ws.activate()
                }
            }
        }
        // a stable anchor for the insight pane (jack delegates are rebuilt on every roster write)
        Item { id: jackAnchor; height: root.barH }
    }

    // ── the active window's title, dim, truncated ───────────────────────────
    Text {
        id: titleText
        x: board.x + board.width + root.kit.cells(2)
        width: Math.max(0, right.x - root.kit.cells(2) - x)
        visible: width >= root.kit.cells(6)
        text: (Hyprland.activeToplevel && Hyprland.activeToplevel.title) ? "" + Hyprland.activeToplevel.title : ""
        color: root.kit.dim
        font: root.kit.font
        textFormat: Text.PlainText
        elide: Text.ElideRight
        maximumLineCount: 1
    }

    // ── the right cells ─────────────────────────────────────────────────────
    Row {
        id: right
        anchors.right: parent.right
        anchors.rightMargin: root.kit.cellW
        height: root.barH
        spacing: root.kit.cellW

        Cell {
            kit: root.kit
            label: "AGT"
            value: root.agentStats.working + "/" + root.agentStats.total
            valueColor: root.agentStats.awaiting > 0 ? root.kit.urgent : root.kit.number
            onActivated: root.openBoard("overview")
        }
        Cell {
            kit: root.kit
            label: "CPU"
            value: root.cpuPct < 0 ? "—" : Math.round(root.cpuPct) + "%"
            valueColor: root.cpuPct < 0 ? root.kit.dim : (root.cpuPct >= 80 ? root.kit.warn : root.kit.number)
            onActivated: root.openBoard("sys")
        }
        Cell {
            kit: root.kit
            label: "$"
            value: root.todayCost === null ? "—" : root.todayCost.toFixed(2)
            valueColor: root.todayCost === null ? root.kit.dim : root.kit.number
            onActivated: root.openBoard("sys")
        }
        Cell {
            kit: root.kit
            value: "!" + root.heraldStats.count
            valueColor: root.heraldStats.summons ? root.kit.urgent
                        : (root.heraldStats.count > 0 ? root.kit.number : root.kit.dim)
            onActivated: root.openBoard("notif")
        }
        Sep { kit: root.kit }
        Cell {
            id: volCell
            kit: root.kit
            label: "VOL"
            value: !root.sinkAudio ? "—" : (root.volMuted ? "mute" : root.volPct + "%")
            valueColor: (!root.sinkAudio || root.volMuted) ? root.kit.dim : root.kit.number
            lit: root.openPane === "vol"
            onActivated: root.togglePane("vol", volCell, true)
            onWheeled: function (d) { root.nudgeVol(root.sinkAudio, d > 0 ? 5 : -5) }
        }
        Cell {
            id: btCell
            kit: root.kit
            label: "BT"
            value: !root.btAdapter ? "—"
                   : (root.btDev ? Kit.padR("" + (root.btDev.name || root.btDev.deviceName || "dev"), 8).trim()
                                 : (root.btOn ? "on" : "off"))
            valueColor: root.btDev ? root.kit.ink : root.kit.dim
            lit: root.openPane === "bt"
            onActivated: root.togglePane("bt", btCell, true)
        }
        Cell {
            id: netCell
            kit: root.kit
            label: "NET"
            value: root.netKind === "down" ? "off" : root.netKind
            valueColor: root.netKind === "down" ? root.kit.warn : root.kit.ink
            lit: root.openPane === "net"
            onActivated: root.togglePane("net", netCell, true)
        }
        Cell {
            id: batCell
            visible: root.battAvail
            kit: root.kit
            label: "BAT"
            value: root.battPct + "%" + (root.battCharging ? "+" : "")
            valueColor: root.battCharging ? root.kit.number
                        : (root.battPct <= 10 ? root.kit.urgent : (root.battPct <= 20 ? root.kit.warn : root.kit.number))
            lit: root.openPane === "bat"
            onActivated: root.togglePane("bat", batCell, true)
        }
        Sep { kit: root.kit }
        Cell {
            id: trayCell
            visible: root.trayCount > 0
            kit: root.kit
            label: "TRAY"
            value: "" + root.trayCount
            valueColor: root.kit.number
            lit: root.openPane === "tray"
            onActivated: root.togglePane("tray", trayCell, true)
        }
        Cell {
            kit: root.kit
            label: "RICE"
            value: root.modeWord(root.livery.riceMode)
            valueColor: root.modeColor(root.livery.riceMode)
            onActivated: root.bridge.toggleRiceMode()
        }
        Sep { kit: root.kit }
        Cell {
            id: clockCell
            kit: root.kit
            value: root.clockText
            valueColor: root.openPane === "clock" ? root.kit.title : root.kit.ink
            onActivated: root.togglePane("clock", clockCell, true)
        }
    }

    // ════════════════════════════════════════════════════════════════════════
    // PANE BODIES — the kit's Pane, content on the cell grid
    // ════════════════════════════════════════════════════════════════════════
    // a list row: [n] lamp name ……… gloss ; hover → select band, index amber
    component ListRow: Item {
        id: lr
        required property var kit
        property int cols: 30
        property string idx: ""
        property string lamp: ""
        property color lampColor: kit.dim
        property string name: ""
        property color nameColor: kit.ink
        property string gloss: ""
        property color glossColor: kit.dim
        signal activated()
        signal secondary()
        readonly property bool hot: lrMa.containsMouse
        readonly property int idxCells: idx.length > 0 ? idx.length + 3 : 0
        readonly property int lampCells: lamp.length > 0 ? 2 : 0
        readonly property int glossCells: Math.min(gloss.length, Math.max(0, cols - idxCells - lampCells - 6))
        width: kit.cells(cols)
        height: kit.cellH
        Rectangle { anchors.fill: parent; color: lr.kit.select; visible: lr.hot }
        Text {
            visible: lr.idx.length > 0
            text: "[" + lr.idx + "]"
            color: lr.hot ? lr.kit.hot : lr.kit.dim
            font: lr.kit.font; textFormat: Text.PlainText
        }
        Text {
            x: lr.kit.cells(lr.idxCells)
            visible: lr.lamp.length > 0
            text: lr.lamp
            color: lr.lampColor
            font: lr.kit.font; textFormat: Text.PlainText
        }
        Text {
            x: lr.kit.cells(lr.idxCells + lr.lampCells)
            // +2px: a run of n glyphs is a hair wider than round(n·cellW); never elide a fit
            width: lr.kit.cells(lr.cols - lr.idxCells - lr.lampCells - (lr.glossCells > 0 ? lr.glossCells + 1 : 0)) + 2
            text: lr.name.replace(/\s+/g, " ")
            color: lr.hot ? lr.kit.match : lr.nameColor
            elide: Text.ElideRight
            font: lr.kit.font; textFormat: Text.PlainText
        }
        Text {
            anchors.right: parent.right
            visible: lr.glossCells > 0
            width: lr.kit.cells(lr.glossCells) + 2
            horizontalAlignment: Text.AlignRight
            elide: Text.ElideRight
            text: lr.gloss.replace(/\s+/g, " ")
            color: lr.hot ? lr.kit.match : lr.glossColor
            font: lr.kit.font; textFormat: Text.PlainText
        }
        MouseArea {
            id: lrMa
            anchors.fill: parent
            hoverEnabled: true
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            cursorShape: Qt.PointingHandCursor
            onClicked: function (m) { if (m.button === Qt.RightButton) lr.secondary(); else lr.activated() }
        }
    }
    // `─ label ─────` inside a pane
    component Rule: Text {
        required property var kit
        property int cols: 30
        property string label: ""
        text: label.length > 0 ? "─ " + label + " " + Kit.rep("─", Math.max(0, cols - label.length - 3))
                               : Kit.rep("─", cols)
        color: kit.dim
        font: kit.font
        textFormat: Text.PlainText
    }
    // `lbl  ████▌░░░░  62%` — a gauge line; click sets, wheel nudges
    component GaugeLine: Item {
        id: gl
        required property var kit
        property int cols: 30
        property string label: ""
        property real frac: 0
        property string valueText: ""
        property color fillColor: kit.ink
        property color valueColor: kit.number
        property bool interactive: false
        signal setFrac(real f)
        signal wheeled(int delta)
        signal labelClicked()
        readonly property int gaugeCells: Math.max(4, cols - 5 - 6)
        readonly property var g: kit.gaugeText(frac, gaugeCells)
        width: kit.cells(cols)
        height: kit.cellH
        Text {
            text: gl.label
            color: labelMa.containsMouse ? gl.kit.title : gl.kit.dim
            font: gl.kit.font; textFormat: Text.PlainText
            MouseArea {
                id: labelMa
                anchors.fill: parent
                enabled: gl.interactive
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: gl.labelClicked()
            }
        }
        Item {
            x: gl.kit.cells(5)
            width: gl.kit.cells(gl.gaugeCells)
            height: gl.kit.cellH
            Text {
                text: gl.g.fill
                color: gl.fillColor
                font: gl.kit.font; textFormat: Text.PlainText
            }
            Text {
                x: gl.kit.cells(gl.g.fill.length)
                text: gl.g.track
                color: gl.kit.dim
                font: gl.kit.font; textFormat: Text.PlainText
            }
            MouseArea {
                anchors.fill: parent
                enabled: gl.interactive
                cursorShape: Qt.PointingHandCursor
                onClicked: function (m) { gl.setFrac(Math.max(0, Math.min(1, m.x / width))) }
                onWheel: function (w) { gl.wheeled(w.angleDelta.y) }
            }
        }
        Text {
            anchors.right: parent.right
            text: gl.valueText
            color: gl.valueColor
            font: gl.kit.font; textFormat: Text.PlainText
        }
    }
    // a plain one-line note (honest empty states)
    component Note: Text {
        required property var kit
        property int cols: 30
        width: kit.cells(cols)
        color: kit.dim
        font: kit.font
        textFormat: Text.PlainText
        elide: Text.ElideRight
    }

    // ── VOL ─────────────────────────────────────────────────────────────────
    readonly property int volCols: 40
    readonly property int volRows: !root.sinkAudio && !root.srcAudio ? 1
        : 2 + 1 + Math.max(1, sinkRoster.length) + 1 + Math.max(1, sourceRoster.length) + 1
    Component {
        id: volPane
        Use {
            kit: root.kit; helper: "Pane"
            props: ({
                title: "volume", focused: true, animateOnCreate: true, cols: root.volCols,
                stat: Qt.binding(() => root.sinkAudio ? (root.volMuted ? "mute" : root.volPct + "%") : ""),
                rows: Qt.binding(() => root.volRows), content: volBody })
        }
    }
    Component {
        id: volBody
        Column {
            Note { kit: root.kit; cols: root.volCols; visible: !root.sinkAudio && !root.srcAudio
                   text: "no audio device — pipewire not ready" }
            GaugeLine {
                visible: !!root.sinkAudio
                kit: root.kit; cols: root.volCols; interactive: true
                label: "out"; frac: root.volPct / 100
                valueText: root.volMuted ? "mute" : root.volPct + "%"
                fillColor: root.volMuted ? root.kit.dim : root.kit.ink
                valueColor: root.volMuted ? root.kit.dim : root.kit.number
                onLabelClicked: root.toggleMute(root.sinkAudio)
                onSetFrac: function (f) { root.setVol(root.sinkAudio, f) }
                onWheeled: function (d) { root.nudgeVol(root.sinkAudio, d > 0 ? 5 : -5) }
            }
            GaugeLine {
                visible: !!root.sinkAudio || !!root.srcAudio
                kit: root.kit; cols: root.volCols; interactive: !!root.srcAudio
                label: "in"; frac: root.micPct / 100
                valueText: !root.srcAudio ? "—" : (root.micMuted ? "mute" : root.micPct + "%")
                fillColor: root.micMuted ? root.kit.dim : root.kit.ink
                valueColor: root.micMuted ? root.kit.dim : root.kit.number
                onLabelClicked: root.toggleMute(root.srcAudio)
                onSetFrac: function (f) { root.setVol(root.srcAudio, f) }
                onWheeled: function (d) { root.nudgeVol(root.srcAudio, d > 0 ? 5 : -5) }
            }
            Rule { kit: root.kit; cols: root.volCols; label: "out"; visible: !!root.sinkAudio || !!root.srcAudio }
            Note { kit: root.kit; cols: root.volCols; visible: (!!root.sinkAudio || !!root.srcAudio) && root.sinkRoster.length === 0
                   text: "no output device" }
            Repeater {
                model: root.sinkRoster
                delegate: ListRow {
                    required property var modelData
                    required property int index
                    kit: root.kit; cols: root.volCols
                    idx: "" + (index + 1)
                    lamp: modelData.current ? "●" : "○"
                    lampColor: modelData.current ? root.kit.title : root.kit.dim
                    name: modelData.name
                    nameColor: modelData.current ? root.kit.bright : root.kit.ink
                    gloss: modelData.gloss
                    onActivated: root.pickSink(modelData.key)
                }
            }
            Rule { kit: root.kit; cols: root.volCols; label: "in"; visible: !!root.sinkAudio || !!root.srcAudio }
            Note { kit: root.kit; cols: root.volCols; visible: (!!root.sinkAudio || !!root.srcAudio) && root.sourceRoster.length === 0
                   text: "no input device" }
            Repeater {
                model: root.sourceRoster
                delegate: ListRow {
                    required property var modelData
                    required property int index
                    kit: root.kit; cols: root.volCols
                    idx: "" + (index + 1)
                    lamp: modelData.current ? "●" : "○"
                    lampColor: modelData.current ? root.kit.title : root.kit.dim
                    name: modelData.name
                    nameColor: modelData.current ? root.kit.bright : root.kit.ink
                    gloss: modelData.gloss
                    onActivated: root.pickSource(modelData.key)
                }
            }
            ListRow {
                visible: !!root.sinkAudio || !!root.srcAudio
                kit: root.kit; cols: root.volCols
                idx: "m"; name: "mixer"; gloss: "pavucontrol"
                onActivated: root.openMixer()
            }
        }
    }

    // ── BT ──────────────────────────────────────────────────────────────────
    readonly property int btCols: 36
    readonly property int btRows: !root.btAdapter ? 1 : 1 + 1 + Math.max(1, btRoster.length)
    Component {
        id: btPane
        Use {
            kit: root.kit; helper: "Pane"
            props: ({
                title: "bluetooth", focused: true, animateOnCreate: true, cols: root.btCols,
                stat: Qt.binding(() => !root.btAdapter ? "" : (root.btOn ? "on" : "off")),
                statColor: Qt.binding(() => root.btOn ? root.kit.title : root.kit.dim),
                rows: Qt.binding(() => root.btRows), content: btBody })
        }
    }
    Component {
        id: btBody
        Column {
            Note { kit: root.kit; cols: root.btCols; visible: !root.btAdapter; text: "no adapter" }
            ListRow {
                visible: !!root.btAdapter
                kit: root.kit; cols: root.btCols
                idx: "p"; name: "power"; gloss: root.btOn ? "on" : "off"
                glossColor: root.btOn ? root.kit.title : root.kit.dim
                onActivated: root.toggleBtPower()
            }
            Rule { kit: root.kit; cols: root.btCols; label: "devices"; visible: !!root.btAdapter }
            Note { kit: root.kit; cols: root.btCols; visible: !!root.btAdapter && root.btRoster.length === 0
                   text: root.btOn ? "no paired device" : "adapter off" }
            Repeater {
                model: root.btAdapter ? root.btRoster : []
                delegate: ListRow {
                    required property var modelData
                    required property int index
                    kit: root.kit; cols: root.btCols
                    idx: "" + (index + 1)
                    lamp: modelData.current ? "●" : "○"
                    lampColor: modelData.current ? root.kit.title : root.kit.dim
                    name: modelData.name
                    gloss: modelData.gloss
                    onActivated: root.pickBt(modelData.key)
                }
            }
        }
    }

    // ── NET ─────────────────────────────────────────────────────────────────
    readonly property int netCols: 40
    readonly property bool netBackend: Networking.backend !== NetworkBackendType.None
    readonly property int netRows: !root.netBackend ? 2
        : 1 + (root.wiredDev ? 1 : 0) + (root.wifiDev ? 1 + 1 + Math.max(1, wifiList.length) : 0)
    Component {
        id: netPane
        Use {
            kit: root.kit; helper: "Pane"
            props: ({
                title: "network", focused: true, animateOnCreate: true, cols: root.netCols,
                stat: Qt.binding(() => root.netWord()),
                statColor: Qt.binding(() => root.netWord() === "full" ? root.kit.number : root.kit.warn),
                rows: Qt.binding(() => root.netRows), content: netBody })
        }
    }
    Component {
        id: netBody
        Column {
            ListRow {
                kit: root.kit; cols: root.netCols
                name: "route"
                gloss: root.netKind === "down" ? "no default route" : "default via " + root.netKind
                glossColor: root.netKind === "down" ? root.kit.warn : root.kit.ink
            }
            Note { kit: root.kit; cols: root.netCols; visible: !root.netBackend; text: "no network backend" }
            ListRow {
                visible: root.netBackend && !!root.wiredDev
                kit: root.kit; cols: root.netCols
                lamp: root.wiredDev && root.wiredDev.connected ? "●" : "○"
                lampColor: root.wiredDev && root.wiredDev.connected ? root.kit.title : root.kit.dim
                name: "wired " + (root.wiredDev ? root.wiredDev.name : "")
                gloss: root.wiredDev && root.wiredDev.connected ? "connected" : "no link"
            }
            ListRow {
                visible: root.netBackend && !!root.wifiDev
                kit: root.kit; cols: root.netCols
                idx: "w"; name: "wifi " + (root.wifiDev ? root.wifiDev.name : "")
                gloss: Networking.wifiEnabled ? "on" : "off"
                glossColor: Networking.wifiEnabled ? root.kit.title : root.kit.dim
                onActivated: Networking.wifiEnabled = !Networking.wifiEnabled
            }
            Rule { kit: root.kit; cols: root.netCols; label: "networks"; visible: root.netBackend && !!root.wifiDev }
            Note { kit: root.kit; cols: root.netCols
                   visible: root.netBackend && !!root.wifiDev && root.wifiList.length === 0
                   text: Networking.wifiEnabled ? "scanning…" : "wifi off" }
            Repeater {
                model: root.netBackend ? root.wifiList : []
                delegate: ListRow {
                    required property var modelData
                    required property int index
                    readonly property int sig: root.sigPct(modelData.signalStrength)
                    readonly property bool needsPsk: !modelData.known && root.secured(modelData.security)
                    kit: root.kit; cols: root.netCols
                    idx: "" + (index + 1)
                    lamp: modelData.connected ? "●" : (modelData.stateChanging ? "◐" : "○")
                    lampColor: modelData.connected ? root.kit.title : root.kit.dim
                    name: "" + modelData.name
                    gloss: (needsPsk ? "psk: nmtui  " : "") + "▁▂▃▄▅▆▇█".charAt(Math.round(sig / 100 * 7)) + " " + sig
                    glossColor: needsPsk ? root.kit.dim : root.kit.number
                    onActivated: {
                        if (modelData.connected) modelData.disconnect()
                        else if (!needsPsk) modelData.connect()
                    }
                    onSecondary: if (modelData.known) modelData.forget()
                }
            }
        }
    }

    // ── BAT ─────────────────────────────────────────────────────────────────
    readonly property int batCols: 30
    Component {
        id: batPane
        Use {
            kit: root.kit; helper: "Pane"
            props: ({
                title: "battery", focused: true, animateOnCreate: true, cols: root.batCols, rows: 2,
                stat: Qt.binding(() => root.battFull ? "full" : (root.battCharging ? "charging" : "")),
                content: batBody })
        }
    }
    Component {
        id: batBody
        Column {
            GaugeLine {
                kit: root.kit; cols: root.batCols
                label: "bat"; frac: root.battPct / 100
                valueText: root.battPct + "%"
                fillColor: root.battCharging ? root.kit.ink
                           : (root.battPct <= 10 ? root.kit.urgent : (root.battPct <= 20 ? root.kit.warn : root.kit.ink))
            }
            ListRow {
                kit: root.kit; cols: root.batCols
                name: root.battCharging ? "to full" : "left"
                gloss: root.battTime() || "—"
                glossColor: root.kit.number
            }
        }
    }

    // ── TRAY ────────────────────────────────────────────────────────────────
    readonly property int trayCols: 40
    Component {
        id: trayPane
        Use {
            kit: root.kit; helper: "Pane"
            props: ({
                title: "tray", focused: true, animateOnCreate: true, cols: root.trayCols,
                stat: Qt.binding(() => "" + root.trayCount),
                rows: Qt.binding(() => Math.max(1, root.trayCount)), content: trayBody })
        }
    }
    Component {
        id: trayBody
        Column {
            Repeater {
                model: SystemTray.items
                delegate: Item {
                    id: trow
                    required property var modelData
                    required property int index
                    readonly property bool attn: !!modelData && modelData.status === Status.NeedsAttention
                    width: root.kit.cells(root.trayCols)
                    height: root.kit.cellH
                    ListRow {
                        kit: root.kit; cols: root.trayCols
                        idx: "" + (trow.index + 1)
                        name: {
                            var m = trow.modelData
                            var t = (m && m.title) ? ("" + m.title).trim() : ""
                            return t.length > 0 ? t : ((m && m.id) ? "" + m.id : "item")
                        }
                        nameColor: trow.attn ? root.kit.urgent : root.kit.ink
                        gloss: (trow.modelData && trow.modelData.tooltipTitle) ? "" + trow.modelData.tooltipTitle : ""
                        onActivated: {
                            var m = trow.modelData
                            if (!m) return
                            if (m.hasMenu) trayMenu.open(); else m.activate()
                        }
                        onSecondary: {
                            var m = trow.modelData
                            if (!m) return
                            if (m.hasMenu) trayMenu.open(); else m.secondaryActivate()
                        }
                    }
                    QsMenuAnchor {
                        id: trayMenu
                        menu: trow.modelData ? trow.modelData.menu : null
                        anchor.item: trow
                        anchor.edges: Edges.Bottom
                        anchor.gravity: Edges.Bottom
                    }
                }
            }
        }
    }

    // ── CLOCK → the calendar slot (sonata's mechanism), else the date ──────
    readonly property bool calendarReady: !!root.stagingEngine
        && root.stagingEngine.resolveSong(root.livery.songName, "calendar") !== ""
    // A calendar slot that resolves but fails to build (a helper type it
    // cannot reach) leaves an empty WidgetSlot; the date pane stands in.
    Component {
        id: clockPane
        Item {
            id: cp
            readonly property bool calUp: !!calLoader.item && calLoader.item.implicitHeight > 0
            implicitWidth: calUp ? calLoader.implicitWidth : dateLoader.implicitWidth
            implicitHeight: calUp ? calLoader.implicitHeight : dateLoader.implicitHeight
            Loader { id: calLoader; active: root.calendarReady; visible: cp.calUp; sourceComponent: calSlot }
            Loader { id: dateLoader; active: !cp.calUp; sourceComponent: datePane }
        }
    }
    Component {
        id: calSlot
        WidgetSlot {
            livery: root.livery
            bridge: root.bridge
            stagingEngine: root.stagingEngine
            slot: "calendar"
        }
    }
    Component {
        id: datePane
        Use {
            kit: root.kit; helper: "Pane"
            props: ({ title: "date", focused: true, animateOnCreate: true, cols: 24, rows: 1,
                      stat: Qt.binding(() => root.clockText.slice(0, 5)), content: dateBody })
        }
    }
    Component {
        id: dateBody
        Note { kit: root.kit; cols: 24; color: root.kit.ink
               text: Qt.formatDate(root.now, "ddd dd MMM yyyy") }
    }

    // ── JACK INSIGHT (intent §3.4) ──────────────────────────────────────────
    readonly property int jackCols: 38
    readonly property var insightJack: {
        var i = root.jackIndex(root.insightId)
        return i >= 0 ? root.jackModel[i] : null
    }
    readonly property var insightRow: {
        var d = root.nowDoc
        if (!d || d.by !== "workspace" || !Array.isArray(d.rows)) return null
        for (var i = 0; i < d.rows.length; i++)
            if (d.rows[i] && d.rows[i].workspace === root.insightId) return d.rows[i]
        return null
    }
    readonly property var insightTokens: {
        var r = root.insightRow
        var t = (r && r.total) ? r.total.tokens : null
        return t ? (t["in"] || 0) + (t.out || 0) : null
    }
    readonly property int jackRows: (insightRow ? 4 : 1) + 1
        + Math.max(1, insightJack ? insightJack.sessions.length : 0) + 1 + 1
    Component {
        id: jackPane
        Use {
            kit: root.kit; helper: "Pane"
            props: ({
                title: Qt.binding(() => "jack " + root.insightId
                    + (root.insightJack && root.insightJack.project ? " · " + root.insightJack.project : "")),
                stat: Qt.binding(() => root.insightJack ? root.insightJack.live + " live" : ""),
                animateOnCreate: true, cols: root.jackCols,
                rows: Qt.binding(() => root.jackRows), content: jackBody })
        }
    }
    Component {
        id: jackBody
        Column {
            Note { kit: root.kit; cols: root.jackCols; visible: !root.insightRow
                   text: "no usage data — bridge not wired" }
            // tokens: sparkline of the per-minute series + the in+out total
            Item {
                visible: !!root.insightRow
                width: root.kit.cells(root.jackCols); height: root.kit.cellH
                Text { text: "tok"; color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText }
                Text {
                    x: root.kit.cells(5)
                    text: root.insightTokens === null ? "no usage source"
                          : root.kit.sparkText(root.insightRow && root.insightRow.series ? root.insightRow.series.tokens : [], 22, 0)
                    color: root.insightTokens === null ? root.kit.dim : root.kit.ink
                    font: root.kit.font; textFormat: Text.PlainText
                }
                Text {
                    anchors.right: parent.right
                    text: root.fmtTok(root.insightTokens)
                    color: root.insightTokens === null ? root.kit.dim : root.kit.number
                    font: root.kit.font; textFormat: Text.PlainText
                }
            }
            // cost: a partial total is a floor, drawn orange
            Item {
                visible: !!root.insightRow
                width: root.kit.cells(root.jackCols); height: root.kit.cellH
                readonly property var tot: root.insightRow ? root.insightRow.total : null
                readonly property bool known: !!tot && typeof tot.costUsd === "number"
                readonly property bool partial: !!tot && tot.costPartial === true
                Text { text: "$"; color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText }
                Text {
                    x: root.kit.cells(5)
                    visible: parent.partial
                    text: "partial"
                    color: root.kit.warn
                    font: root.kit.font; textFormat: Text.PlainText
                }
                Text {
                    anchors.right: parent.right
                    text: !parent.known ? "—" : (parent.partial ? "≥" : "") + parent.tot.costUsd.toFixed(2)
                    color: !parent.known ? root.kit.dim : (parent.partial ? root.kit.warn : root.kit.number)
                    font: root.kit.font; textFormat: Text.PlainText
                }
            }
            GaugeLine {
                visible: !!root.insightRow
                kit: root.kit; cols: root.jackCols
                label: "cpu"
                frac: root.insightRow && root.insightRow.now ? (root.insightRow.now.cpuPct || 0) / 100 : 0
                valueText: root.insightRow && root.insightRow.now ? Math.round(root.insightRow.now.cpuPct || 0) + "%" : "—"
            }
            GaugeLine {
                visible: !!root.insightRow
                kit: root.kit; cols: root.jackCols
                label: "mem"
                frac: root.insightRow && root.insightRow.now && root.memTotal > 0
                      ? (root.insightRow.now.rssBytes || 0) / root.memTotal : 0
                valueText: root.insightRow && root.insightRow.now ? root.fmtBytes(root.insightRow.now.rssBytes) : "—"
            }
            Rule { kit: root.kit; cols: root.jackCols; label: "sessions" }
            Note { kit: root.kit; cols: root.jackCols
                   visible: !root.insightJack || root.insightJack.sessions.length === 0
                   text: "no conducted session" }
            Repeater {
                model: root.insightJack ? root.insightJack.sessions : []
                delegate: ListRow {
                    required property var modelData
                    kit: root.kit; cols: root.jackCols
                    lamp: root.kit.lampGlyph(modelData.state)
                    lampColor: root.kit.lampColor(modelData.state)
                    name: modelData.name + "  " + modelData.agent
                    gloss: modelData.state
                    onActivated: root.bridge.focusSession(modelData.id)
                }
            }
            Rule { kit: root.kit; cols: root.jackCols }
            ListRow {
                kit: root.kit; cols: root.jackCols
                idx: "b"; name: "btop"; gloss: "the real one"
                onActivated: root.launchBtop()
            }
        }
    }
}
