// wallpaper.qml — cadenza's `wallpaper` slot: the BOARD, live (intent §3.9).
//
// The cover is the song's circuit board (CoverPcb.qml generates it, one fixed
// seed, and the static PNG under this widget is the same board). This body
// adds the one thing a still cannot have: LIGHT MOVING ON THE COPPER.
//
// ── What it draws ───────────────────────────────────────────────────────────
//   · the copper — CoverPcb, instantiated by URL and left to paint itself
//     (its own Canvas, rasterised once). This file never re-draws a track.
//   · the light — ONE delegate per pulse (`component Light` below), each one a
//     head, a handful of tail rectangles and its node ring. No Canvas: a
//     full-surface Canvas damages the WHOLE output every frame (2.07 M px at
//     1080p, plus the texture upload) and that starves the GUI thread — opening
//     the launcher or the dock then crawls. Items damage only themselves.
//     Every pulse is still a RECORD in `pulses` and all the geometry is still
//     widgets/Trace.js's (`Trace.light` returns the rectangles).
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
// The copper is rasterised once (`Canvas.Image`, CoverPcb's own target), and
// its board is generated in CoverPcb's WorkerScript (seconds of JS per output,
// never on the GUI thread), so the light starts when the board arrives. The
// light costs one small binding pass per frame per pulse (~0.003 ms of engine
// work for the whole board, measured) and damages only the rectangles it
// occupies — a few thousand px, not the screen. The one dial left is
// `interval`: it sets how often the light MOVES, not how much is repainted.
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
    property int interval: 33      // how often the light MOVES (ms), not a repaint cost
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
    // required property needs. `z: 1` keeps the board under every light item.
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

    // ── the claimed nodes: one ring per live session ────────────────────────
    // A node is a DWELL-shaped record (Trace.dwell) that the scheduler never
    // touches: it is not in `pulses`, so it costs no budget — it is a claim on
    // a pad, not a pulse. The ring comes from a hash of the sessionId, so an
    // agent keeps the same pad across reloads; the phase is hashed too, so two
    // nodes never breathe in lockstep.
    //
    // Built by an explicit call, not a binding: Trace.dwell draws from the
    // shared PRNG, and a binding that quietly consumed draws every time it
    // re-evaluated would shift the scheduler's sequence under it.
    property var nodes: []
    function rebuildNodes() {
        var out = []
        var n = root.index && root.index.near ? root.index.near.length : 0
        if (!n) { root.nodes = out; return }
        for (var id in root.liveStates) {
            var d = Trace.dwell(root.rngState, Trace.hash(id) % n,
                "" + root.kit.lampColor(root.liveStates[id]),
                root.liveStates[id] === "working" ? 0.55 : 0.35)
            d.phase = (Trace.hash(id) % 63) / 10
            out.push(d)
        }
        root.nodes = out
    }
    onIndexChanged: rebuildNodes()
    onKitChanged: rebuildNodes()

    FileView {
        id: sessionsFile
        path: root.aoideRoot + "/state/stage/sessions.json"
        watchChanges: true
        printErrors: false
        onFileChanged: reload()
        onTextChanged: { root.rows = Trace.sessions(sessionsFile.text()); root.rebuildNodes() }
        onLoadFailed: { root.rows = []; root.rebuildNodes() }
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
    // THE LIGHT — one record array, one clock, one delegate per light
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
    // yet (the board arrives from CoverPcb's worker, seconds after the first
    // paint).
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
            if (root.index && root.board) {
                root.pulses = Trace.reconcile(root.pulses, root.liveStates, {
                    rng: root.rngState, board: root.board, index: root.index,
                    hues: root.hues, opts: { cap: root.pulseCap, idleFloor: root.idleFloor }
                })
                root.pulses = Trace.advance(root.pulses, dt)
            }
            // no repaint call: the delegates below re-read `clock` themselves.
            // A Canvas here would damage the whole screen 30 times a second —
            // see the note over the Repeaters.
        }
    }

    // ── the light: ONE delegate per pulse, never one full-screen canvas ─────
    // A Canvas repaints its WHOLE surface: at 1920x1080 that damages 2.07 M px
    // and re-uploads the texture, every frame — measured at ~0 cost in the
    // engine (0.003 ms/frame) and all of it in the rasterise, which is what made
    // opening a surface (the launcher, the dock) crawl: the GUI thread never
    // idled. Here the damaged area is the light itself — a head, six tail
    // rectangles and a ring per pulse, a few thousand px in total.
    //
    // `model` is a COUNT, not the array. The count changes only when the SET of
    // pulses does, so no delegate is rebuilt per frame; the records themselves
    // mutate in place and the delegate re-reads them off `clock`.
    Repeater {
        model: root.pulses.length
        delegate: Light {
            required property int index
            kit: root.kit
            board: root.board
            record: root.pulses[index]
            clock: root.clock
        }
    }

    // one ring per live session — a node claimed, breathing, in its state's
    // colour (more processes, more lit nodes)
    Repeater {
        model: root.nodes.length
        delegate: Light {
            required property int index
            kit: root.kit
            board: root.board
            record: root.nodes[index]
            clock: root.clock
        }
    }

    // The delegate, declared once and used by both Repeaters above. Inline on
    // purpose: cadenza resolves nothing by type name (design/kit.md §1), and a
    // local component needs no qmldir and no URL.
    //
    // `face` is the whole geometry for one frame (Trace.light): touching
    // `clock` is what re-evaluates it, since the record's fields mutate in
    // place and QML cannot observe that.
    component Light: Item {
        id: one
        // above the board (the Loader is z 1): a Repeater is not a visual item,
        // so the stacking has to be declared on the delegate itself.
        z: 2
        required property var kit
        required property var board
        required property var record
        required property real clock
        readonly property var face: Trace.light(one.record, 6, one.board, one.clock)

        Rectangle {   // the node: this pulse's arrival bloom, or a dwell's breath
            readonly property var g: one.face ? one.face.ring : null
            visible: g !== null
            x: g ? g.x - width / 2 : 0
            y: g ? g.y - height / 2 : 0
            width: g ? g.r * 2 : 0
            height: width
            radius: width / 2
            color: "transparent"
            border.width: g ? g.stroke : 0
            border.color: one.face ? one.face.hue : "transparent"
            opacity: g ? g.alpha : 0
            antialiasing: true
        }
        Repeater {    // the tail: one flat rectangle per straight piece
            model: one.face ? one.face.tail.length : 0
            delegate: Rectangle {
                required property int index
                readonly property var s: one.face.tail[index]
                x: s.x
                y: s.y
                width: s.w
                height: s.h
                rotation: s.rot
                transformOrigin: Item.Left
                color: one.face.hue
                opacity: s.alpha
                visible: s.alpha > 0.01
                antialiasing: true
            }
        }
        Rectangle {   // the head
            readonly property var h: one.face ? one.face.head : null
            visible: h !== null && h.alpha > 0.01
            x: h ? h.x - width / 2 : 0
            y: h ? h.y - height / 2 : 0
            width: h ? h.r * 2 : 0
            height: width
            radius: width / 2
            color: one.kit.bright
            opacity: h ? h.alpha : 0
            antialiasing: true
        }
    }
}
