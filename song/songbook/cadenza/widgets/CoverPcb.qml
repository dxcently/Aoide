// CoverPcb.qml — the cover: a routed circuit board on the CRT black
// (intent §3.9). An uppercase helper, never a slot: nothing loads it live.
// The preview canvas shoots it at each monitor's size into
// `cover/pcb-<w>x<h>.png` (cover/README.md), and `lyra cover set` stages the
// PNG. Regenerating after a palette change is one shot.
//
// What it draws, generated (not drawn) from a fixed seed:
//   · tracks — orthogonal runs with 45° bends, `select` (base02), 2.5 px;
//   · pads and vias — hollow rings in `dim` (base03), filled with the ground;
//   · buses — groups at a fixed pitch that bend together (mitred offsets, so
//     the pitch holds through every 45° bend), entering from the board edge
//     or fanning out of a pad row and converging to a tighter pitch;
//   · singles — short pad-to-via and via-to-via hops filling the margins.
// Every track avoids every other (an occupancy grid with clearance), and every
// track that ends on the board ends on a pad or a via. Density falls off
// toward the centre, so windows sit on quiet black.
//
// Why one Canvas: the board is a few hundred polylines and rings painted
// ONCE. A Canvas rasterises them into one image and keeps no scene-graph
// object per track; Shapes would keep ~1000 live ShapePaths (each its own
// geometry, re-tessellated on any change) for a picture that never changes.
// It repaints only when its size has settled, a board lands, or the livery
// changes.
//
// The generator (CoverPcbWorker.js) is about three seconds of JS per output,
// so it runs in a WorkerScript, never on the GUI thread. The canvas strokes
// the board `Canvas.Threaded` (on the GUI thread that is another ~100 ms), on
// the one render thread the engine shares between every Threaded canvas, behind
// any Pane glows queued before it. It paints the ground until the first board
// arrives, then the board: `board` is null only until then. A size change
// restarts a 250ms `settle` Timer, and only the settled size is asked of the
// worker, which computes one board at a time: the last board stays on the
// canvas until the new one lands, and an answer to a superseded size is
// dropped.
//
// Deterministic: one seed (`seed` below), a local PRNG (mulberry32), no
// Math.random, no clock. The same size and seed give the same pixels.
// Static: no animation; the one Timer is the resize settle. No text.
import QtQuick
import QtQml.WorkerScript
import "Kit.js" as Kit

Item {
    id: root

    required property var livery
    required property var bridge

    width: parent ? parent.width : 1920
    height: parent ? parent.height : 1080

    FontMetrics { id: fm; font: Kit.bodyFont(13) }
    readonly property var kit: Kit.make(root.livery, fm)

    // "cadenza" on the board: the one seed every render uses.
    readonly property int seed: 0x00CADE2A

    property var board: null
    property string wantKey: ""

    Timer {
        id: settle
        interval: 250
        onTriggered: {
            var w = Math.round(root.width), h = Math.round(root.height)
            var key = w + "x" + h + ":" + root.seed
            if (key !== root.wantKey) {
                root.wantKey = key
                worker.sendMessage({ key: key, w: w, h: h, seed: root.seed })
            }
            canvas.requestPaint()
        }
    }

    WorkerScript {
        id: worker
        source: "CoverPcbWorker.js"
        onMessage: function (m) {
            if (m.key !== root.wantKey) return
            root.board = m.board
            canvas.requestPaint()
        }
    }

    Canvas {
        id: canvas
        anchors.fill: parent
        renderTarget: Canvas.Image
        renderStrategy: Canvas.Threaded
        onPaint: {
            if (settle.running) return      // a resize repaints by itself; `settle` paints the settled size
            var ctx = getContext("2d")
            var bd = root.board
            ctx.reset()
            ctx.fillStyle = "" + root.kit.ground
            ctx.fillRect(0, 0, width, height)
            if (!bd) return
            ctx.lineCap = "round"
            ctx.lineJoin = "round"
            ctx.lineWidth = bd.tw
            ctx.strokeStyle = "" + root.kit.select
            ctx.beginPath()
            for (var t = 0; t < bd.tracks.length; t++) {
                var p = bd.tracks[t]
                ctx.moveTo(p[0], p[1])
                for (var i = 2; i + 1 < p.length; i += 2) ctx.lineTo(p[i], p[i + 1])
            }
            ctx.stroke()
            // rings: the drilled hole is the ground, the annulus is dim phosphor
            ctx.lineWidth = bd.ring
            ctx.fillStyle = "" + root.kit.ground
            ctx.strokeStyle = "" + root.kit.dim
            for (var r = 0; r < bd.rings.length; r++) {
                var g = bd.rings[r]
                ctx.beginPath()
                ctx.arc(g[0], g[1], g[2], 0, 2 * Math.PI)
                ctx.fill()
                ctx.stroke()
            }
        }
    }

    Component.onCompleted: settle.start()
    onWidthChanged: settle.restart()
    onHeightChanged: settle.restart()
    onKitChanged: canvas.requestPaint()
}
