// wallpaper.qml — cadenza's `wallpaper` slot: the BOARD, live (intent §3.9).
//
// The cover is the song's circuit board (CoverPcb.qml generates it, one fixed
// seed, and the static PNG under this widget is the same board). This body
// adds the one thing a still cannot have: LIGHT MOVING ON THE COPPER.
//
// ── What it draws ───────────────────────────────────────────────────────────
//   · the copper — CoverPcb, instantiated by URL and left to paint itself
//     (its own Canvas, rasterised once). This file never re-draws a track.
//   · the light — ONE transparent Canvas over it, repainted on one timer. No
//     Item per pulse, no animation object per effect: every pulse is a RECORD
//     in `pulses` and every frame is a single pass over them (widgets/Trace.js
//     is the whole engine — the walker, the scheduler, the step, the easing).
//   · the claimed nodes — one board ring per live session, in its state's
//     colour (the kit's own table: `kit.lampColor`), breathing. More processes,
//     more lit nodes. The board's own rings stay `dim` underneath.
//
// ── The machine drives the light (intent §3.9) ──────────────────────────────
// `state/stage/sessions.json` says who is alive and what they are doing;
// `state/stage/hooks.json` says who just DID something (its `updatedAt`
// advancing between two reads — the bar's own activity rule, `Trace.advanced`).
// The budget follows both (Trace.want): a floor of light with nothing running
// — a tube never goes fully dark — then two pulses per working agent and one
// per awaiting, capped. A firing agent earns a burst on its own node.
//
// ── Colours: the key, not a palette of its own ──────────────────────────────
// working = `title` (bright phosphor), awaiting = `urgent` (termui red), idle
// = `dim`. Amber (`hot`) is deliberately NOT used: intent §2 gives it to the
// ONE live element, and there are many pulses by design.
//
// ── Cost, stated plainly ────────────────────────────────────────────────────
// The copper is rasterised once (`Canvas.Image`, CoverPcb's own target). The
// light canvas is a full-surface transparent repaint at `interval` ms — the
// default is a CPU-rasterised Canvas because that is the target this shell is
// already proven on. If a venue finds it heavy, `interval` is the one dial
// (33 → 50 halves the cost); `Canvas.FramebufferObject` is the untested
// alternative, not the default, because a failed FBO canvas draws nothing at
// all on the wrong graphics API.
//
// Preview: WallpaperPreview.qml (the canvas harness) — knobs below.
import QtQuick
import Quickshell
import Quickshell.Io
import "Kit.js" as Kit
import "Trace.js" as Trace

Item {
    id: root

    required property var livery
    required property var bridge

    // ── preview knobs (WallpaperPreview.qml sets these; the live path never) ─
    property real rate: 1.0        // clock multiplier — 0.2 catches a slow leg
    property int interval: 33      // the light's frame
    property int pulseCap: 14      // Trace.want's cap
    property int idleFloor: 2      // …and its idle floor
    property int seedWanted: 0     // 0 → seeded from the clock, so each boot differs

    anchors.fill: parent

    FontMetrics { id: fm; font: Kit.bodyFont(13) }
    readonly property var kit: Kit.make(root.livery, fm)

    readonly property string aoideRoot:
        Quickshell.env("AOIDE_ROOT") || (Quickshell.env("HOME") + "/.aoide")

    // ════════════════════════════════════════════════════════════════════════
    // THE COPPER — CoverPcb, by URL, with the song's own loading idiom
    // ════════════════════════════════════════════════════════════════════════
    // `setSource` with initial properties (bar.qml's `Use` and BarPreview's own
    // loader do exactly this for helpers that declare `required property var
    // livery`): the props are applied at object creation, which is what a
    // required property needs. `z: 1` keeps the board under the light canvas.
    Loader {
        id: cover
        anchors.fill: parent
        z: 1
        Component.onCompleted: setSource(root.kit.helper("CoverPcb"), {
            livery: root.livery,
            bridge: root.bridge
        })
    }
    readonly property var board: cover.item ? cover.item.board : null

    // The board's little graph: which ring each track end lands on. Built once
    // per board (a few hundred tracks), never per frame.
    readonly property var index: {
        if (!board) return null
        return Trace.index(board)
    }

    // ════════════════════════════════════════════════════════════════════════
    // THE FEED — who is alive, who just moved
    // ════════════════════════════════════════════════════════════════════════
    property var rows: []
    readonly property var liveStates: Trace.live(rows)
    property var stamps: ({})
    property bool stampsReady: false

    FileView {
        id: sessionsFile
        path: root.aoideRoot + "/state/stage/sessions.json"
        watchChanges: true
        printErrors: false
        onFileChanged: reload()
        onTextChanged: root.rows = Trace.sessions(sessionsFile.text())
        onLoadFailed: root.rows = []
    }
    FileView {
        id: hookFile
        path: root.aoideRoot + "/state/stage/hooks.json"
        watchChanges: true
        printErrors: false
        onFileChanged: reload()
        onTextChanged: {
            var next = Trace.stamps(hookFile.text())
            var ids = Trace.advanced(root.stamps, next, !root.stampsReady)
            root.stamps = next
            root.stampsReady = true
            if (ids.length) root.ignite(ids)
        }
        onLoadFailed: { root.stamps = ({}); root.stampsReady = false }
    }

    // ════════════════════════════════════════════════════════════════════════
    // THE LIGHT — one record array, one clock, one paint pass
    // ════════════════════════════════════════════════════════════════════════
    property var pulses: []
    property real clock: 0          // seconds since start, for the node breath
    property var rngState: Trace.rng(root.seedWanted || (Date.now() & 0x7fffffff))

    readonly property var hues: ({
        working: "" + root.kit.title,
        awaiting: "" + root.kit.urgent,
        idle: "" + root.kit.dim
    })

    function ignite(ids) {
        // A firing agent earns a burst on its own node: two fast pulses that
        // run INTO it (so the eye lands on the pad that did something) and then
        // a dwell, the pad answering where it stands. Capped like everything
        // else — and the dwell carries its own lifetime (Trace.dwell), so a
        // burst can never park a node in the budget forever.
        if (!root.index || !root.board) return
        for (var i = 0; i < ids.length && root.pulses.length < root.pulseCap; i++) {
            var ring = root.ringOf(ids[i])
            if (ring < 0 || root.index.near[ring].length === 0) continue
            var st = root.liveStates[ids[i]] || "working"
            for (var k = 0; k < 2 && root.pulses.length < root.pulseCap; k++) {
                var np = root.spawnAt(ring, st, true)
                if (np) root.pulses = root.pulses.concat([np])
            }
            if (root.pulses.length < root.pulseCap)
                root.pulses = root.pulses.concat([
                    Trace.dwell(root.rngState, ring, root.hues.working, 0.55, 5)
                ])
        }
    }

    // An agent keeps its node across reloads and restarts: the ring comes from
    // a hash of its sessionId, never from its position in a list.
    function ringOf(id) {
        var n = root.index ? root.index.near.length : 0
        return n > 0 ? Trace.hash("" + id) % n : -1
    }

    // One pulse aimed at a ring, or the idle roam when there is nothing to aim
    // at. Both go through Trace.spawn — the variation is all in the arguments.
    // The roam falls back to no pulse at all when the board has no wired track
    // yet (the board arrives a frame after the first paint).
    function spawnAt(ring, state, urgent) {
        var r = root.rngState
        if (!root.index || !root.board) return null
        if (ring < 0 || root.index.near[ring].length === 0) {
            var wired = root.index.wired
            var t = wired[Trace.pickIdx(r, wired.length)]
            if (t === undefined) return null
            var ends = root.index.ends[t]
            return Trace.spawn(r, {
                track: t, pts: root.board.tracks[t],
                dir: ends[1] >= 0 ? 1 : -1,
                ring: ends[1] >= 0 ? ends[1] : ends[0],
                ringOut: ends[0] >= 0 ? ends[0] : -1,
                hue: root.hues.idle, alpha: 0.3, speed: 70
            })
        }
        var near = root.index.near[ring]
        var track = near[Trace.pickIdx(r, near.length)]
        var e = root.index.ends[track]
        var busy = state === "working" || urgent
        return Trace.spawn(r, {
            track: track, pts: root.board.tracks[track],
            dir: e[1] === ring ? 1 : -1,
            ring: ring,
            ringOut: e[0] === ring ? e[1] : e[0],
            state: state,
            hue: busy ? root.hues.working : root.hues.awaiting,
            speed: urgent ? 320 : (busy ? 190 : 100),
            alpha: urgent ? 1.0 : (busy ? 0.8 : 0.5),
            legs: urgent ? 1 : undefined
        })
    }

    Timer {
        interval: root.interval
        repeat: true
        running: true
        onTriggered: {
            var dt = (interval / 1000) * root.rate
            root.clock += dt
            if (root.index && root.board)
                root.pulses = Trace.reconcile(root.pulses, root.liveStates, {
                    rng: root.rngState, board: root.board, index: root.index,
                    hues: root.hues, opts: { cap: root.pulseCap, idleFloor: root.idleFloor }
                })
            root.pulses = Trace.advance(root.pulses, dt)
            light.requestPaint()
        }
    }

    Canvas {
        id: light
        anchors.fill: parent
        z: 2
        renderTarget: Canvas.Image
        onPaint: root.paintLight(getContext("2d"))
    }
    onWidthChanged: light.requestPaint()
    onHeightChanged: light.requestPaint()
    onKitChanged: light.requestPaint()

    // ── the one paint pass ─────────────────────────────────────────────────
    // Tail, head, bloom, node — every pulse the same way, so a new look is a
    // new NUMBER in the record, never a new branch here.
    function paintLight(ctx) {
        ctx.reset()
        var bd = root.board
        if (!bd) return
        ctx.lineCap = "round"

        // 1. the claimed nodes: one ring per live session, breathing
        var st = root.liveStates
        var rw = bd.ring
        ctx.lineWidth = rw
        for (var id in st) {
            var ring = root.index && root.index.near.length ? bd.rings[Trace.hash(id) % bd.rings.length] : null
            if (!ring) continue
            var col = st[id]
            var breathe = 0.5 + 0.5 * Math.sin(root.clock * (col === "working" ? 2.2 : 1.1) + Trace.hash(id) % 7)
            ctx.globalAlpha = col === "working" ? 0.35 + 0.45 * breathe : 0.22 + 0.3 * breathe
            ctx.strokeStyle = "" + root.kit.lampColor(col)
            ctx.beginPath()
            ctx.arc(ring[0], ring[1], ring[2] + rw, 0, 2 * Math.PI)
            ctx.stroke()
        }

        // 2. the light on the copper
        var P = root.pulses
        for (var i = 0; i < P.length; i++) {
            var p = P[i]
            if (p.dwell) continue
            ctx.strokeStyle = p.hue
            // tail: a handful of segments behind the head, fading — a comet on
            // copper, not a gradient (cheap, and it follows every 45° bend)
            var N = 6
            for (var k = N; k >= 1; k--) {
                var a = Trace.tail(p, (k - 1) * p.trail / N)
                var b = Trace.tail(p, k * p.trail / N)
                ctx.globalAlpha = Math.max(0, p.alpha * (1 - k / (N + 1)) * 0.8)
                ctx.lineWidth = Math.max(1, p.head * (1 - k / (N + 1.5)))
                ctx.beginPath()
                ctx.moveTo(a.x, a.y)
                ctx.lineTo(b.x, b.y)
                ctx.stroke()
            }
            var hd = Trace.head(p)
            ctx.globalAlpha = Math.min(1, p.alpha)
            ctx.fillStyle = "" + root.kit.bright
            ctx.beginPath()
            ctx.arc(hd.x, hd.y, p.head, 0, 2 * Math.PI)
            ctx.fill()
            // 3. the node it just ran into
            if (p.bloom > 0 && p.bloomRing >= 0 && p.bloomRing < bd.rings.length) {
                var g = bd.rings[p.bloomRing]
                ctx.globalAlpha = p.bloom * 0.7
                ctx.strokeStyle = p.hue
                ctx.lineWidth = rw * (1 + p.bloom)
                ctx.beginPath()
                ctx.arc(g[0], g[1], g[2] + 1.5 + p.bloom * 5, 0, 2 * Math.PI)
                ctx.stroke()
            }
        }
        // 4. a dwelling pulse: the node itself, breathing, no traveller
        for (i = 0; i < P.length; i++) {
            var q = P[i]
            if (!q.dwell || q.ring < 0 || q.ring >= bd.rings.length) continue
            var rg = bd.rings[q.ring]
            ctx.globalAlpha = 0.25 + 0.5 * q.bloom
            ctx.strokeStyle = q.hue
            ctx.lineWidth = rw * 1.6
            ctx.beginPath()
            ctx.arc(rg[0], rg[1], rg[2] + 2 + q.bloom * 3, 0, 2 * Math.PI)
            ctx.stroke()
        }
        ctx.globalAlpha = 1
    }
}
