import QtQuick
import Quickshell
import Quickshell.Io

// ── THE LEDGER · a claude.ai usage stele ──────────────────────────────────────
// A live view of how hard the account is leaning on claude.ai — the plan's
// rolling 5-hour window, the weekly caps (overall + per-model), any extra-usage
// credits — over a LOCAL this-machine token/cost estimate. Reads the file the
// `aoide usage` poller writes (state/usage.json, schema §0); the gadget only
// exists while that file does, so it is effectively toggled on by
// `aoide usage enable` (which starts the poller). Fifth of the pantheon; it
// wears the shared marble-stele grammar but stands in its OWN order and hue:
//
//   · ORDER      — a LEDGER stele, re-skinned to read as CLAUDE. Its signature is
//                  base09 — in the light sonata scheme a CLAY / kiln-fired orange
//                  (#c06a35), the palette's warmest, most Claude-coral note: the
//                  spark, the inset keyline, the box-drawing frame and the frieze
//                  all run clay, where the Conductor ran gold, the Meters teal, the
//                  Power murex and the Terminals aegean — plainly its own building.
//                  Its frieze is a BEAD-AND-REEL astragal (beads alternating with
//                  paired reels) — a counting-off ornament, fitting a meter of
//                  consumption; distinct from the Conductor's baton-and-downbeat,
//                  the Meters' triglyph, the Power's egg-and-dart.
//   · MUSIC      — the CLAUDE SPARK ❋ (U+274B, a rayed sunburst — the Anthropic
//                  mark) crowns it, in place of the old segno. A gold 𝄂 closes the
//                  score.
//   · TERMINAL   — box-drawing frames the panel; each cap is the dock's shared
//                  [▓▓░░] shade-glyph gauge (LiveryState.ctxBar), tinted clay
//                  and swung to terracotta past 85% (ctxColor) like every other
//                  meter in the house.
//
// GRACEFUL DEGRADE + LIVE-READY: today the poller's `live` block is a bare
// { ok:false, error } stub — so the live section renders a single quiet, dim
// "unavailable" note (NOT a red alarm), never empty/zero gauges, while the LOCAL
// tile (real data) always renders. Every live sub-field is existence-guarded, so
// the day `live.ok` flips true and fiveHour/sevenDay/… arrive, the plan/weekly/
// credit gauges light up with NO further change. Height is content-driven, so the
// degraded panel is compact, not a big empty marble box. All colour flows from
// `notes`; hard corners (radius 0) everywhere.
Item {
    id: gadget

    required property var notes            // palette roles
    property string usagePath: "/home/khoa/Aoide/state/usage.json"

    implicitWidth: 360   // fallback only; the dock stack sets width = root.gadgetW (360)
    // content-driven height: the frame hugs whatever the sections need, so the
    // degraded (live-stub) state is a short panel, not an empty box.
    implicitHeight: body.implicitHeight + 26

    // type voices ──────────────────────────────────────────────────────────────
    readonly property string faceSerif:  "Noto Serif"               // carved marble
    readonly property string faceMono:   "JetBrainsMono Nerd Font"  // the terminal
    readonly property string faceMusic:  "Noto Music"               // notation
    readonly property string faceSymbol: "Noto Sans Symbols 2"      // the Claude spark ❋

    // this temple's signature accent — base09, its own among the pantheon. In the
    // LIGHT sonata scheme base09 resolves to #c06a35, a "clay / kiln-fired orange":
    // already the palette's warmest, most Claude-coral note that does NOT collide
    // with urgent (base08 #b0472f terracotta, which ctxColor swings to past 85%).
    // So the Claude re-skin stays palette-driven on base09 — no off-palette coral.
    // (A literal Claude clay ≈ #d97757 is noted for the vision-check, not applied.)
    readonly property color signature: notes.base09

    readonly property int cells: 14
    readonly property real urgentAt: 85

    property real nowMs: Date.now()

    function withA(cstr, a) {                      // alpha on a role string
        var c = Qt.darker(cstr, 1.0)
        return Qt.rgba(c.r, c.g, c.b, a)
    }

    // ── PARSED USAGE ────────────────────────────────────────────────────────────
    // The whole document, or {} when the file is missing/empty/garbage. hasData
    // gates the ENTIRE gadget's visibility (see AoidePanel) so a host without the
    // poller enabled shows nothing at all.
    property var usage: ({})
    // TICK HOOK — `usage` is reassigned to a FRESH object (JSON.parse always
    // allocates a new one) at the end of every parseUsage() call, and
    // parseUsage() only ever runs from usageFile.onLoaded (itself fired by
    // FileView's watchChanges → onFileChanged → reload()) or
    // Component.onCompleted's initial read. So onUsageChanged fires exactly
    // once per aoide poller write (or parse attempt) — never on an idle
    // frame — which makes it the one signal that means "aoide just pushed
    // fresh data", as opposed to e.g. the 30s nowMs countdown timer below,
    // which fires on a clock regardless of whether the file changed at all.
    // clef (the ❋ spark, declared further down) listens here for its tick.
    onUsageChanged: clef.tick()
    readonly property bool hasData: !!(usage && (usage.local || usage.live))

    // the live block, and its ok flag — every field below is guarded off THIS,
    // so the section is inert (a note only) until the day `ok` turns true.
    readonly property var live: (usage && usage.live) ? usage.live : ({})
    readonly property bool liveOk: live && live.ok === true
    readonly property string liveError: (live && live.error) ? live.error : "live fetch not enabled"

    // guarded live sub-blocks — null unless liveOk AND the field is actually there
    readonly property var fiveHour:  (liveOk && live.fiveHour)       ? live.fiveHour       : null
    readonly property var sevenDay:  (liveOk && live.sevenDay)       ? live.sevenDay       : null
    readonly property var opusWk:    (liveOk && live.sevenDayOpus)   ? live.sevenDayOpus   : null
    readonly property var sonnetWk:  (liveOk && live.sevenDaySonnet) ? live.sevenDaySonnet : null
    readonly property var extra:     (liveOk && live.extraUsage)     ? live.extraUsage     : null
    readonly property bool creditsOn: !!(extra && extra.isEnabled === true)

    // the local estimate (real data now) — null when the block is absent
    readonly property var local:     (usage && usage.local)         ? usage.local         : null
    readonly property var localToday: (local && local.today)        ? local.today         : null
    readonly property var localWeek:  (local && local.week)         ? local.week          : null
    readonly property string localNote: (local && local.note) ? local.note : "local estimate, this machine only"

    function utilOf(block) { return (block && block.utilization !== undefined) ? block.utilization : 0 }
    function money(v) { return (v === undefined || v === null) ? "—" : "$" + Number(v).toFixed(2) }

    // ── FILE: the usage document, hot-reloaded on change ────────────────────────
    function parseUsage() {
        try {
            var t = usageFile.text()
            if (!t || t.trim().length === 0) { gadget.usage = ({}); return }
            gadget.usage = JSON.parse(t)
        } catch (e) {
            gadget.usage = ({})
        }
    }
    FileView {
        id: usageFile
        path: gadget.usagePath
        watchChanges: true
        blockLoading: false
        printErrors: false
        onLoaded: gadget.parseUsage()
        onFileChanged: reload()
    }
    // ticks the reset countdowns without a re-read
    Timer { interval: 30000; running: gadget.liveOk; repeat: true; onTriggered: gadget.nowMs = Date.now() }
    Component.onCompleted: parseUsage()

    // cast shadow — lifts the stele off any marble wallpaper ─────────────────────
    Rectangle {
        anchors.fill: stele
        anchors.leftMargin: 4; anchors.topMargin: 5
        anchors.rightMargin: -4; anchors.bottomMargin: -5
        radius: 0
        color: gadget.withA(notes.paletteFg, 0.22)
    }

    // the stele ──────────────────────────────────────────────────────────────────
    Rectangle {
        id: stele
        anchors.fill: parent
        radius: 0
        color: notes.paletteBg
        border.color: notes.paletteFg
        border.width: 2

        Rectangle {                                   // inset keyline — CLAY (Claude)
            anchors.fill: parent; anchors.margins: 4
            radius: 0; color: "transparent"
            border.color: gadget.signature; border.width: 1
        }

        // ── reusable utilization cap — the shared [▓▓░░] gauge ───────────────────
        component UsageMeter : Item {
            id: um
            property string label: ""
            property var block: null            // { utilization, resetsAt }
            property int barCells: gadget.cells
            property bool compact: false
            readonly property bool present: !!(block && block.utilization !== undefined)
            readonly property real util: gadget.utilOf(block)
            readonly property bool urgent: util >= gadget.urgentAt
            width: parent ? parent.width : 0
            visible: present
            height: present ? (compact ? 26 : 34) : 0

            Text {                                    // section label (serif)
                id: umName
                anchors.left: parent.left
                anchors.top: parent.top
                text: um.label
                font.family: gadget.faceSerif
                font.pixelSize: um.compact ? 12 : 14
                font.weight: Font.Medium; font.letterSpacing: 2
                color: notes.paletteFg
            }
            Text {                                    // util %, tallied in gold / terracotta
                id: umPct
                anchors.right: parent.right
                anchors.baseline: umName.baseline
                text: Math.round(um.util) + "%"
                font.family: gadget.faceMono; font.pixelSize: um.compact ? 12 : 15
                color: um.urgent ? notes.paletteUrgent : notes.paletteAccent
            }
            Text {                                    // reset countdown (small, dim, italic)
                anchors.left: umName.right; anchors.leftMargin: 8
                anchors.right: umPct.left; anchors.rightMargin: 6   // bounded — never bleeds under the %
                anchors.baseline: umName.baseline
                text: gadget.notes.usageResetIn(um.block ? um.block.resetsAt : "", gadget.nowMs)
                elide: Text.ElideRight
                horizontalAlignment: Text.AlignLeft
                font.family: gadget.faceSerif; font.italic: true
                font.pixelSize: 10
                color: gadget.withA(notes.paletteFg, 0.5)
            }
            Text {                                    // the shared shade-glyph gauge
                anchors.left: parent.left; anchors.right: parent.right
                anchors.bottom: parent.bottom; anchors.bottomMargin: um.compact ? 0 : 2
                text: gadget.notes.ctxBar(um.util, um.barCells)
                font.family: gadget.faceMono; font.pixelSize: um.compact ? 12 : 14
                fontSizeMode: Text.HorizontalFit
                horizontalAlignment: Text.AlignLeft
                color: gadget.notes.ctxColor(um.util, gadget.signature)
            }
        }

        // ── CONTENT — a top-down score; the frame hugs it (content-driven height) ─
        Column {
            id: body
            anchors.left: parent.left; anchors.right: parent.right
            anchors.top: parent.top
            anchors.margins: 13
            spacing: 8

            // ── ENTABLATURE: Claude spark · CLAUDE · [ claude.ai ] ───────────────
            Item {
                id: head
                width: parent.width
                height: 38

                Rectangle {                           // deeper marble band
                    anchors.fill: parent; anchors.bottomMargin: 5
                    color: gadget.withA(notes.paletteFg, 0.05)
                }
                Text {                                // the Claude spark — a rayed
                    id: clef                          // sunburst, crowning the stele
                    anchors.left: parent.left
                    anchors.verticalCenter: parent.verticalCenter
                    anchors.verticalCenterOffset: -2
                    text: "❋"; font.family: gadget.faceSymbol; font.pixelSize: 26   // U+274B — the Claude mark
                    color: gadget.signature
                    transformOrigin: Item.Center
                    rotation: 0
                    scale: 1.0

                    // TICK — a quick full spin + scale pulse, fired once per
                    // aoide poller write (see gadget.onUsageChanged above):
                    // the spark's MOTION is a live-update heartbeat, distinct
                    // from the slow idle shimmer below which just keeps it
                    // from reading as inert between pushes. from/to are set
                    // imperatively right before each restart (rather than
                    // bound to clef.rotation) so a tick mid-spin extends
                    // smoothly instead of snapping back to 0.
                    function tick() {
                        tickSpin.from = clef.rotation
                        tickSpin.to = clef.rotation + 360
                        tickSpin.restart()
                        tickPulse.restart()
                    }
                    RotationAnimation {
                        id: tickSpin
                        target: clef; property: "rotation"
                        duration: 620
                        easing.type: Easing.OutCubic
                    }
                    SequentialAnimation {
                        id: tickPulse
                        NumberAnimation { target: clef; property: "scale"; to: 1.12; duration: 260; easing.type: Easing.OutCubic }
                        NumberAnimation { target: clef; property: "scale"; to: 1.0;  duration: 340; easing.type: Easing.InCubic }
                    }

                    // idle — a slow, subtle opacity shimmer so the spark
                    // never sits fully static between pushes; deliberately
                    // much quieter than the tick, so the tick still reads
                    // as the distinct "just updated" event.
                    SequentialAnimation on opacity {
                        loops: Animation.Infinite
                        NumberAnimation { to: 0.72; duration: 1800; easing.type: Easing.InOutSine }
                        NumberAnimation { to: 1.0;  duration: 1800; easing.type: Easing.InOutSine }
                    }
                }
                Text {
                    anchors.left: clef.right; anchors.leftMargin: 8
                    anchors.verticalCenter: parent.verticalCenter
                    text: "CLAUDE"
                    font.family: gadget.faceSerif; font.pixelSize: 19
                    font.weight: Font.DemiBold; font.letterSpacing: 4
                    color: notes.paletteFg
                }
                Text {                                // the source, unmistakably
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    text: "[ claude.ai ]"
                    font.family: gadget.faceMono; font.pixelSize: 11
                    color: gadget.withA(gadget.signature, 0.95)
                }
            }

            // ── bead-and-reel astragal frieze, in clay ──────────────────────────
            Canvas {
                id: frieze
                width: parent.width; height: 13
                onWidthChanged: requestPaint()
                onPaint: {
                    var ctx = getContext("2d")
                    ctx.reset()
                    ctx.clearRect(0, 0, width, height)
                    ctx.strokeStyle = gadget.signature
                    ctx.lineWidth = 1.4
                    var u = height / 5
                    var top = u * 0.5, bot = height - u * 0.5, mid = (top + bot) / 2
                    ctx.beginPath()                    // the two rails
                    ctx.moveTo(0, top); ctx.lineTo(width, top)
                    ctx.moveTo(0, bot); ctx.lineTo(width, bot)
                    ctx.stroke()
                    var period = height * 1.1, r = (bot - top) / 2 - 1.5
                    for (var x = 0; x < width; x += period) {
                        var bx = x + period * 0.3       // the bead (a circle)
                        ctx.beginPath()
                        ctx.arc(bx, mid, r, 0, 2 * Math.PI)
                        ctx.stroke()
                        var rx = x + period * 0.75      // the reel (paired short bars)
                        ctx.beginPath()
                        ctx.moveTo(rx - 1.5, top + 1); ctx.lineTo(rx - 1.5, bot - 1)
                        ctx.moveTo(rx + 1.5, top + 1); ctx.lineTo(rx + 1.5, bot - 1)
                        ctx.stroke()
                    }
                }
            }

            // ── box-drawing top frame: ┌─┤ ♪ usage ├────┐ ────────────────────────
            Item {
                id: topFrame
                width: parent.width; height: 15

                Text {
                    id: tfL
                    anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter
                    text: "┌─┤ ♪ usage ├"
                    font.family: gadget.faceMono; font.pixelSize: 11
                    color: gadget.withA(gadget.signature, 0.95)
                }
                Text {
                    id: tfR
                    anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                    text: "┐"
                    font.family: gadget.faceMono; font.pixelSize: 11
                    color: gadget.withA(gadget.signature, 0.95)
                }
                Rectangle {
                    anchors.left: tfL.right; anchors.right: tfR.left
                    anchors.leftMargin: 2; anchors.rightMargin: 2
                    anchors.verticalCenter: parent.verticalCenter
                    anchors.verticalCenterOffset: 1
                    height: 1; color: gadget.withA(gadget.signature, 0.55)
                }
            }

            // ── LIVE SECTION — gauges when ok, a quiet note otherwise ────────────
            Column {
                id: liveSection
                width: parent.width
                spacing: 8

                // ok: the plan / weekly / per-model caps + credits (each self-hides
                // when its block is absent, so partial live payloads degrade too)
                UsageMeter { label: "PLAN · 5-HOUR"; block: gadget.fiveHour }
                UsageMeter { label: "WEEKLY";        block: gadget.sevenDay }
                UsageMeter { label: "OPUS · WK";     block: gadget.opusWk;   barCells: 10; compact: true }
                UsageMeter { label: "SONNET · WK";   block: gadget.sonnetWk; barCells: 10; compact: true }

                Item {                                // extra-usage credits, when enabled
                    width: parent.width; height: gadget.creditsOn ? 16 : 0
                    visible: gadget.creditsOn
                    Text {
                        anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter
                        text: "credits"
                        font.family: gadget.faceSerif; font.pixelSize: 12
                        font.letterSpacing: 2; color: notes.paletteFg
                    }
                    Text {
                        anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                        text: gadget.extra
                              ? (gadget.money(gadget.extra.usedCredits)
                                 + " / " + gadget.money(gadget.extra.monthlyLimit))
                              : ""
                        font.family: gadget.faceMono; font.pixelSize: 11
                        color: gadget.withA(gadget.signature, 0.95)
                    }
                }

                // NOT ok (today's reality): one dim, quiet note — NOT a red alarm,
                // and NEVER an empty/zero gauge. Lights out entirely the day live
                // turns on (the gauges above take over with no other change).
                Item {
                    width: parent.width
                    visible: !gadget.liveOk
                    height: visible ? 40 : 0
                    Column {
                        anchors.left: parent.left; anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 3
                        Text {
                            text: "live usage unavailable"
                            font.family: gadget.faceSerif; font.italic: true
                            font.pixelSize: 12
                            color: gadget.withA(notes.paletteFg, 0.5)
                        }
                        Text {
                            width: parent.width
                            text: "— " + gadget.liveError + " · enable the live fetch"
                            elide: Text.ElideRight
                            font.family: gadget.faceMono; font.pixelSize: 10
                            color: gadget.withA(notes.paletteFg, 0.38)
                        }
                    }
                }
            }

            // ── hairline rule between the live headline and the local ledger ─────
            Rectangle {
                width: parent.width; height: 1
                color: gadget.withA(gadget.signature, 0.30)
            }

            // ── LOCAL TILE — this-machine estimate (real data now) ───────────────
            Column {
                id: localTile
                width: parent.width
                spacing: 4

                // a ledger row — label · tokens · ~$cost
                component LocalRow : Item {
                    property string tag: ""
                    property var span: null
                    width: parent ? parent.width : 0
                    height: 18
                    Text {
                        id: lrName
                        anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter
                        text: tag
                        font.family: gadget.faceSerif; font.pixelSize: 13
                        font.weight: Font.Medium; font.letterSpacing: 2
                        color: notes.paletteFg
                    }
                    Text {                            // ~$cost, tallied in gold
                        id: lrCost
                        anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                        text: span ? ("~" + gadget.money(span.costUsd)) : "—"
                        font.family: gadget.faceMono; font.pixelSize: 12
                        color: notes.paletteAccent
                    }
                    Text {                            // token count, compacted (686M …)
                        anchors.right: lrCost.left; anchors.rightMargin: 10
                        anchors.verticalCenter: parent.verticalCenter
                        text: span ? (gadget.notes.ctxCompact(span.tokens) + " tok") : ""
                        font.family: gadget.faceMono; font.pixelSize: 11
                        color: gadget.withA(gadget.signature, 0.9)
                    }
                }

                LocalRow { tag: "TODAY";     span: gadget.localToday }
                LocalRow { tag: "THIS WEEK"; span: gadget.localWeek }

                Text {                                // the honest caveat — small + dim
                    width: parent.width
                    text: gadget.localNote
                    elide: Text.ElideRight
                    font.family: gadget.faceSerif; font.italic: true
                    font.pixelSize: 10
                    color: gadget.withA(notes.paletteFg, 0.45)
                }
            }

            // ── box-drawing bottom frame: └─┤ summary ├── 𝄂 ┘ ───────────────────
            Item {
                id: footer
                width: parent.width; height: 20

                Text {
                    id: ffL
                    anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter
                    text: "└─┤ " + (gadget.liveOk && gadget.fiveHour
                                    ? ("5h " + Math.round(gadget.utilOf(gadget.fiveHour)) + "%"
                                       + (gadget.sevenDay ? " · wk " + Math.round(gadget.utilOf(gadget.sevenDay)) + "%" : ""))
                                    : "local est · this machine") + " ├"
                    font.family: gadget.faceMono; font.pixelSize: 11
                    color: gadget.withA(notes.paletteFg, 0.8)
                }
                Text {
                    id: ffCorner
                    anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                    text: "┘"
                    font.family: gadget.faceMono; font.pixelSize: 11
                    color: gadget.withA(gadget.signature, 0.95)
                }
                Text {                                // final barline — gold ink (shared)
                    id: ffBar
                    anchors.right: ffCorner.left; anchors.rightMargin: 4
                    anchors.verticalCenter: parent.verticalCenter
                    anchors.verticalCenterOffset: -1
                    text: "𝄂"
                    font.family: gadget.faceMusic; font.pixelSize: 18
                    color: notes.paletteAccent
                }
                Rectangle {
                    anchors.left: ffL.right; anchors.right: ffBar.left
                    anchors.leftMargin: 2; anchors.rightMargin: 6
                    anchors.verticalCenter: parent.verticalCenter
                    anchors.verticalCenterOffset: 1
                    height: 1; color: gadget.withA(gadget.signature, 0.55)
                }
            }
        }
    }
}
