// BoardSys.qml — the board's SYS tab (intent §3.3): machine and spend on one
// tab.
//
//   ┌─ MACHINE ──────────────────────────── 14:02:30 ┐   now.json totals
//   ┌─ JACKS ───────────────────────────────────── 6 ┐   per-jack sparklines
//   ┌─ TOKENS BY JACK ───────────────────────────────┐   bar chart
//   ┌─ ACCOUNT ──────────────────────────── [r] 3m ──┐   state/usage.json
//
// A HELPER (uppercase — never a slot), loaded by URL from BoardBody.
//
// MACHINE / JACKS / TOKENS read `board.usageNow` — `state/usage/now.json`
// (§C, `by: "workspace"`), which does not exist until S5/S6; until then each
// says `no usage data — bridge not wired`. `tokens: null` and `costUsd: null`
// mean UNKNOWN and draw `—`, never 0; `costPartial` draws the figure orange
// with a `~`. ACCOUNT reads `state/usage.json` (real today) through
// `livery.usagePath`; `[r]` asks the daemon to re-run the poller
// (`bridge.refreshUsage`) — the file watch picks the answer up.
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

    readonly property var now: board.usageNow
    readonly property var rows: (now && now.rows) ? now.rows.slice().sort(function (a, b) {
        return (a.workspace || 0) - (b.workspace || 0) }) : []
    readonly property bool hasNow: rows.length > 0
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
                          rows: Qt.binding(() => root.hasNow ? 3 : 1),
                          content: machineBody })
            }
            Use {
                width: col.width
                kit: root.kit; helper: "Pane"
                props: ({ title: "jacks", glow: "bloom",
                          stat: Qt.binding(() => root.hasNow ? "" + root.rows.length : ""),
                          rows: Qt.binding(() => root.hasNow ? root.rows.length * 2 : 1),
                          content: jacksBody })
            }
            Use {
                width: col.width
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
            Text {
                visible: !root.hasNow
                text: "no usage data — bridge not wired"
                color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText
            }
            Use {
                visible: root.hasNow && root.procOk
                kit: root.kit; helper: "Gauge"
                props: ({ label: "CPU", labelCells: 5, cells: 24, warnAt: 0.6, urgentAt: 0.9,
                          value: Qt.binding(() => Math.min(1, root.cpuTotal / 100)),
                          valueText: Qt.binding(() => root.cpuTotal.toFixed(1) + "%") })
            }
            Row {
                visible: root.hasNow && root.procOk
                Text { text: root.kit.padR("MEM", 5); color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText }
                Text { text: root.board.bytes(root.rssTotal); color: root.kit.number; font: root.kit.font; textFormat: Text.PlainText }
                Text { text: " rss across attributed sessions"; color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText }
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
                text: "no usage data — bridge not wired"
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
                text: "no usage data — bridge not wired"
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
