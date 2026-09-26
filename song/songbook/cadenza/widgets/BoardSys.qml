// BoardSys.qml — the board's SYS tab (intent §3.3): machine and spend on one
// tab.
//
//   ┌─ MACHINE ──────────────────────────────────────┐   /proc CPU + memory
//   ┌─ JACKS ───────────────────────────────────── 6 ┐   per-jack sparklines  ┐ hasJackUsage
//   ┌─ TOKENS BY JACK ───────────────────────────────┐   bar chart            ┘
//   ┌─ ACCOUNT ──────────────────────────── [r] 3m ──┐   state/usage.json
//
// A HELPER (uppercase — never a slot), loaded by URL from BoardBody.
//
// MACHINE is the whole machine's CPU and memory, read from the kernel's own
// ledgers (`/proc/stat`, `/proc/meminfo`) on a 2s tick exactly as sonata's
// meters slot reads them — files, never a process; the tick runs only while
// this tab is loaded and the board is open. A host without them says so.
//
// The per-jack numbers — JACKS, TOKENS BY JACK and MACHINE's attributed /
// unattributed lines — read `board.usageNow` (`state/usage/now.json`, §C,
// `by: "workspace"`), which is not published until S5/S6. They are hidden
// while `board.hasJackUsage` is false. With it on: `tokens: null` and
// `costUsd: null` mean UNKNOWN and draw `—`, never 0; `costPartial` draws
// the figure orange with a `~`. ACCOUNT reads `state/usage.json` (real
// today) through `livery.usagePath`; `[r]` asks the daemon to re-run the
// poller (`bridge.refreshUsage`) — the file watch picks the answer up.
import QtQuick
import Quickshell.Io

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

    readonly property bool jackOn: board.hasJackUsage
    readonly property var now: jackOn ? board.usageNow : null
    readonly property var rows: (now && now.rows) ? now.rows.slice().sort(function (a, b) {
        return (a.workspace || 0) - (b.workspace || 0) }) : []
    readonly property bool hasNow: rows.length > 0

    // ── the machine: /proc, as sonata's meters reads it ───────────────────
    property real cpuPct: -1           // -1 until two samples exist
    property real memUsed: 0           // bytes
    property real memTotal: 0
    property bool procRead: false      // a ledger answered at least once
    property real _prevTotal: -1
    property real _prevIdle: -1
    FileView { id: statFile; path: "/proc/stat"; blockLoading: true; printErrors: false }
    FileView { id: memFile; path: "/proc/meminfo"; blockLoading: true; printErrors: false }
    function sample() {
        statFile.reload(); memFile.reload()
        try {
            var line = ("" + (statFile.text() || "")).split("\n")[0].trim().split(/\s+/)
            if (line[0] === "cpu") {
                var total = 0, idle = 0
                for (var i = 1; i < line.length; i++) {
                    var v = parseInt(line[i]); if (isNaN(v)) continue
                    total += v
                    if (i === 4 || i === 5) idle += v          // idle + iowait
                }
                if (root._prevTotal >= 0 && total > root._prevTotal)
                    root.cpuPct = Math.max(0, Math.min(100, (1 - (idle - root._prevIdle) / (total - root._prevTotal)) * 100))
                root._prevTotal = total; root._prevIdle = idle
                root.procRead = true
            }
        } catch (e) {}
        try {
            var kv = {}, ls = ("" + (memFile.text() || "")).split("\n")
            for (var j = 0; j < ls.length; j++) {
                var m = ls[j].match(/^(\w+):\s+(\d+)/)
                if (m) kv[m[1]] = parseInt(m[2]) * 1024
            }
            if (kv.MemTotal > 0) {
                var avail = kv.MemAvailable !== undefined ? kv.MemAvailable
                          : (kv.MemFree || 0) + (kv.Buffers || 0) + (kv.Cached || 0)
                root.memTotal = kv.MemTotal; root.memUsed = kv.MemTotal - avail
                root.procRead = true
            }
        } catch (e) {}
    }
    Timer {
        interval: 2000; repeat: true; triggeredOnStart: true
        running: root.board.open
        onTriggered: root.sample()
    }
    readonly property bool procOk: !!(now && now.process && now.process.ok)

    function tokensOf(r) {
        var t = r && r.total ? r.total.tokens : null
        if (!t) return null
        return (t["in"] || 0) + (t.out || 0) + (t.cacheRead || 0) + (t.cacheWrite || 0)
    }
    function sum(f) {
        var s = 0, any = false
        for (var i = 0; i < rows.length; i++) { var v = f(rows[i]); if (v !== null && v !== undefined && !isNaN(v)) { s += v; any = true } }
        return any ? s : null
    }
    readonly property real cpuTotal: {
        var s = sum(function (r) { return r.now ? r.now.cpuPct : null }) || 0
        if (now && now.unattributed && now.unattributed.now) s += now.unattributed.now.cpuPct || 0
        return s
    }
    readonly property real rssTotal: {
        var s = sum(function (r) { return r.now ? r.now.rssBytes : null }) || 0
        if (now && now.unattributed && now.unattributed.now) s += now.unattributed.now.rssBytes || 0
        return s
    }
    function money(v) { return (v === null || v === undefined) ? "—" : "$" + Number(v).toFixed(2) }
    function clock(iso) {
        var d = new Date("" + iso)
        if (isNaN(d.getTime())) return ""
        function p(n) { return (n < 10 ? "0" : "") + n }
        return p(d.getHours()) + ":" + p(d.getMinutes()) + ":" + p(d.getSeconds())
    }

    // account (state/usage.json)
    readonly property var acct: board.account
    readonly property var live: (acct && acct.live) ? acct.live : ({})
    readonly property bool liveOk: live.ok === true
    readonly property var local: (acct && acct.local && (acct.local.today || acct.local.week)) ? acct.local : null
    readonly property var caps: {
        if (!liveOk) return []
        var out = []
        if (live.fiveHour) out.push({ l: "5-HOUR", b: live.fiveHour })
        if (live.sevenDay) out.push({ l: "WEEKLY", b: live.sevenDay })
        if (live.sevenDayOpus) out.push({ l: "OPUS WK", b: live.sevenDayOpus })
        if (live.sevenDaySonnet) out.push({ l: "SONNET WK", b: live.sevenDaySonnet })
        return out
    }
    readonly property bool credits: liveOk && !!live.extraUsage && live.extraUsage.isEnabled === true
    property bool refreshing: false
    Timer { id: refreshCool; interval: 4000; onTriggered: root.refreshing = false }
    onAcctChanged: { refreshing = false; refreshCool.stop() }
    function refresh() {
        if (refreshing || !board.bridge || !board.bridge.refreshUsage) return
        refreshing = true; refreshCool.restart()
        board.bridge.refreshUsage()
    }

    function handleKey(e) {
        if (e.key === Qt.Key_R) { refresh(); return true }
        if (e.key === Qt.Key_J || e.key === Qt.Key_Down) { flick.flick(0, -800); return true }
        if (e.key === Qt.Key_K || e.key === Qt.Key_Up) { flick.flick(0, 800); return true }
        return false
    }

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
                props: ({ title: "machine", glow: "bloom",
                          stat: Qt.binding(() => root.hasNow ? root.clock(root.now.sampledAt) : ""),
                          statColor: root.kit.dim,
                          rows: Qt.binding(() => (root.procRead ? 2 : 1) + (root.hasNow ? 2 : 0)),
                          content: machineBody })
            }
            // JACKS + TOKENS BY JACK: hidden until now.json is published
            Use {
                width: col.width
                visible: root.jackOn
                kit: root.kit; helper: "Pane"
                props: ({ title: "jacks", glow: "bloom",
                          stat: Qt.binding(() => root.hasNow ? "" + root.rows.length : ""),
                          rows: Qt.binding(() => root.hasNow ? root.rows.length * 2 : 1),
                          content: jacksBody })
            }
            Use {
                width: col.width
                visible: root.jackOn
                kit: root.kit; helper: "Pane"
                props: ({ title: "tokens by jack", glow: "bloom",
                          rows: Qt.binding(() => root.hasNow ? 7 : 1),
                          content: shareBody })
            }
            Use {
                width: col.width
                kit: root.kit; helper: "Pane"
                props: ({ title: "account", glow: "bloom",
                          stat: Qt.binding(() => root.refreshing ? "[r] …" : "[r] " + (root.acct && root.acct.fetchedAt ? root.board.age(root.acct.fetchedAt) : "—")),
                          statColor: root.kit.dim,
                          rows: Qt.binding(() => !root.acct ? 1 : (root.liveOk ? root.caps.length : 1) + (root.credits ? 1 : 0) + (root.local ? 3 : 0)),
                          content: accountBody })
                MouseArea {
                    // the [r] cut into the rule
                    anchors.right: parent.right; anchors.top: parent.top
                    width: root.kit.cells(10); height: root.kit.cellH
                    cursorShape: Qt.PointingHandCursor
                    onClicked: root.refresh()
                }
            }
        }
    }

    // ══ MACHINE ═══════════════════════════════════════════════════════════
    Component {
        id: machineBody
        Column {
            width: parent ? parent.width : 0
            readonly property int w: root.kit.fit(width)
            // the whole machine, from /proc
            Text {
                visible: !root.procRead
                text: "cpu / memory ledgers unavailable on this host"
                color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText
            }
            Row {
                visible: root.procRead
                Use {
                    kit: root.kit; helper: "Gauge"
                    props: ({ label: "CPU", labelCells: 5, cells: 24, warnAt: 0.6, urgentAt: 0.9,
                              value: Qt.binding(() => Math.max(0, root.cpuPct) / 100),
                              valueText: Qt.binding(() => root.cpuPct < 0 ? "…" : root.cpuPct.toFixed(0) + "%") })
                }
            }
            Row {
                visible: root.procRead
                Use {
                    kit: root.kit; helper: "Gauge"
                    props: ({ label: "MEM", labelCells: 5, cells: 24, warnAt: 0.75, urgentAt: 0.9,
                              value: Qt.binding(() => root.memTotal > 0 ? root.memUsed / root.memTotal : 0) })
                }
                Text {
                    text: "  " + root.board.bytes(root.memUsed) + " / " + root.board.bytes(root.memTotal)
                    color: root.kit.number; font: root.kit.font; textFormat: Text.PlainText
                }
            }
            // per-session attribution (now.json; only with hasJackUsage)
            Row {
                visible: root.hasNow && root.procOk
                Text { text: root.kit.padR("ATTR", 7); color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText }
                Text { text: root.cpuTotal.toFixed(1) + "% · " + root.board.bytes(root.rssTotal); color: root.kit.number; font: root.kit.font; textFormat: Text.PlainText }
                Text { text: "   across attributed sessions"; color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText }
            }
            Text {
                visible: root.hasNow && !root.procOk
                text: "process sampling unavailable on this host"
                color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText
            }
            Row {
                visible: root.hasNow
                Text { text: root.kit.padR("UNATTR", 7); color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText }
                Text {
                    text: root.now && root.now.unattributed && root.now.unattributed.now
                          ? root.now.unattributed.now.cpuPct.toFixed(1) + "% · " + root.board.bytes(root.now.unattributed.now.rssBytes)
                          : "—"
                    color: root.kit.number; font: root.kit.font; textFormat: Text.PlainText
                }
                Text {
                    text: root.now ? "   every " + root.now.intervalSecs + "s · " + root.now.range : ""
                    color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText
                }
            }
        }
    }

    // ══ JACKS — two lines per workspace ═══════════════════════════════════
    Component {
        id: jacksBody
        Column {
            width: parent ? parent.width : 0
            readonly property int w: root.kit.fit(width)
            readonly property int spark: Math.max(6, Math.floor((w - 12 - 2 * 11) / 2))
            Text {
                visible: !root.hasNow
                text: "no usage data yet"
                color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText
            }
            Repeater {
                model: root.rows
                Column {
                    id: jr
                    required property var modelData
                    readonly property var r: modelData
                    readonly property int spark: parent ? parent.spark : 8
                    readonly property var series: r.series || ({})
                    readonly property var tok: root.tokensOf(r)
                    readonly property var cost: r.total ? r.total.costUsd : null
                    readonly property bool partial: !!(r.total && r.total.costPartial)
                    Row {
                        Text { text: root.kit.padR("[" + jr.r.workspace + "]", 5); color: root.kit.path; font: root.kit.font; textFormat: Text.PlainText }
                        Text {
                            text: root.kit.padR(jr.r.project || "—", 7) + " "
                            color: jr.r.project ? root.kit.path : root.kit.dim
                            font: root.kit.font; textFormat: Text.PlainText
                        }
                        Text { text: "CPU "; color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText }
                        Text {
                            text: root.kit.sparkText(jr.series.cpuPct || [], jr.spark, 100)
                            color: root.kit.ink; font: root.kit.font; textFormat: Text.PlainText
                        }
                        Text {
                            text: root.kit.padL(jr.r.now ? jr.r.now.cpuPct.toFixed(1) + "%" : "—", 7)
                            color: root.kit.number; font: root.kit.font; textFormat: Text.PlainText
                        }
                        Text { text: "  MEM "; color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText }
                        Text { text: root.board.bytes(jr.r.now ? jr.r.now.rssBytes : null); color: root.kit.number; font: root.kit.font; textFormat: Text.PlainText }
                    }
                    Row {
                        Text { text: root.kit.padR("", 13); font: root.kit.font; textFormat: Text.PlainText }
                        Text { text: "TOK "; color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText }
                        Text {
                            text: (jr.series.tokens && jr.series.tokens.length)
                                  ? root.kit.sparkText(jr.series.tokens, jr.spark, 0) : root.kit.padR("unknown", jr.spark)
                            color: (jr.series.tokens && jr.series.tokens.length) ? root.kit.ink : root.kit.dim
                            font: root.kit.font; textFormat: Text.PlainText
                        }
                        Text { text: root.kit.padL(root.board.human(jr.tok), 7); color: jr.tok === null ? root.kit.dim : root.kit.number; font: root.kit.font; textFormat: Text.PlainText }
                        Text { text: "  COST "; color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText }
                        Text {
                            text: (jr.partial && jr.cost !== null ? "~" : "") + root.money(jr.cost)
                            color: jr.cost === null ? root.kit.dim : (jr.partial ? root.kit.warn : root.kit.number)
                            font: root.kit.font; textFormat: Text.PlainText
                        }
                    }
                }
            }
        }
    }

    // ══ TOKENS BY JACK — the bar chart ════════════════════════════════════
    Component {
        id: shareBody
        Item {
            width: parent ? parent.width : 0
            implicitHeight: root.hasNow ? root.kit.lines(7) : root.kit.cellH
            Text {
                visible: !root.hasNow
                text: "no usage data yet"
                color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText
            }
            Use {
                visible: root.hasNow
                kit: root.kit; helper: "BarChart"
                props: ({ rows: 5, barCells: 4, gapCells: 2,
                          format: function (v) { return root.board.human(v) },
                          bars: Qt.binding(function () {
                              return root.rows.filter(function (r) { return root.tokensOf(r) !== null })
                                  .map(function (r) { return { label: "[" + r.workspace + "]", value: root.tokensOf(r) } })
                          }) })
            }
        }
    }

    // ══ ACCOUNT — state/usage.json ════════════════════════════════════════
    Component {
        id: accountBody
        Column {
            width: parent ? parent.width : 0
            readonly property int w: root.kit.fit(width)
            Text {
                visible: !root.acct
                text: "no account usage — state/usage.json absent"
                color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText
            }
            Text {
                visible: !!root.acct && !root.liveOk
                width: parent.width; elide: Text.ElideRight
                text: "live: " + (root.live.error || "not enabled")
                color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText
            }
            Repeater {
                model: root.caps
                Row {
                    id: cap
                    required property var modelData
                    Use {
                        kit: root.kit; helper: "Gauge"
                        props: ({ label: cap.modelData.l, labelCells: 10, cells: 22, warnAt: 0.6, urgentAt: 0.85,
                                  value: (cap.modelData.b.utilization || 0) / 100 })
                    }
                    Text {
                        text: "  " + root.board.livery.usageResetIn(cap.modelData.b.resetsAt, root.board.nowMs)
                        color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText
                    }
                }
            }
            Row {
                visible: root.credits
                Text { text: root.kit.padR("CREDITS", 10); color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText }
                Text {
                    text: root.credits ? root.money(root.live.extraUsage.usedCredits) + " / " + root.money(root.live.extraUsage.monthlyLimit) : ""
                    color: root.kit.number; font: root.kit.font; textFormat: Text.PlainText
                }
            }
            Text {
                visible: !!root.local
                text: root.local ? "─ " + (root.local.note || "local estimate") + " " : ""
                color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText
            }
            Repeater {
                model: root.local ? [{ l: "TODAY", v: root.local.today }, { l: "WEEK", v: root.local.week }] : []
                Row {
                    id: lr
                    required property var modelData
                    Text { text: root.kit.padR(lr.modelData.l, 10); color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText }
                    Text {
                        text: root.kit.padL(lr.modelData.v ? root.board.human(lr.modelData.v.tokens) : "—", 8) + " tok  "
                        color: root.kit.number; font: root.kit.font; textFormat: Text.PlainText
                    }
                    Text {
                        text: lr.modelData.v ? "~" + root.money(lr.modelData.v.costUsd) : "—"
                        color: root.kit.number; font: root.kit.font; textFormat: Text.PlainText
                    }
                }
            }
        }
    }
}
