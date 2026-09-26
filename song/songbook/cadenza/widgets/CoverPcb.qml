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
// It repaints only when its size or the livery changes.
//
// Deterministic: one seed (`seed` below), a local PRNG (mulberry32), no
// Math.random, no clock. The same size and seed give the same pixels.
// Static: no timers, no animation. No text.
import QtQuick
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
    property string boardKey: ""

    function ensureBoard() {
        var w = Math.round(root.width), h = Math.round(root.height)
        var key = w + "x" + h + ":" + root.seed
        if (key !== root.boardKey) {
            root.board = root.pcbGenerate(w, h, root.seed)
            root.boardKey = key
        }
        return root.board
    }

    // BEGIN-PCB-GENERATOR (plain JS, no QML: a JS script outside Quickshell can lift it verbatim)
    function pcbGenerate(W, H, seed) {
        // ── PRNG: mulberry32 ───────────────────────────────────────────────
        var s = seed >>> 0
        function rnd() {
            s = (s + 0x6D2B79F5) >>> 0
            var t = s
            t = Math.imul(t ^ (t >>> 15), t | 1)
            t ^= t + Math.imul(t ^ (t >>> 7), t | 61)
            return ((t ^ (t >>> 14)) >>> 0) / 4294967296
        }
        function rr(a, b) { return a + (b - a) * rnd() }
        function ri(a, b) { return Math.floor(rr(a, b + 1)) }

        // ── geometry constants ─────────────────────────────────────────────
        var TW = 2.5            // track width
        var GAP = 5             // copper-to-copper clearance between groups
        var VIA = 3.6           // via ring radius (centre of stroke)
        var PAD = 5.2           // pad ring radius
        var RING = 1.5          // ring stroke
        var S2 = Math.SQRT1_2
        // directions, clockwise from east (screen y grows down)
        var DX = [1, S2, 0, -S2, -1, -S2, 0, S2]
        var DY = [0, S2, 1, S2, 0, -S2, -1, -S2]
        function nx(d) { return -DY[d] }   // the offset normal of a direction
        function ny(d) { return DX[d] }

        // ── density: quiet in the middle, busy at the edges and corners ───
        var cx = W / 2, cy = H / 2
        function edge(x, y) {
            var u = Math.abs(x - cx) / cx, v = Math.abs(y - cy) / cy
            return Math.pow(Math.pow(u, 3) + Math.pow(v, 3), 1 / 3)
        }
        var Q0 = 0.50, Q1 = 0.96
        function dens(x, y) {
            var e = (edge(x, y) - Q0) / (Q1 - Q0)
            return e < 0 ? 0 : (e > 1 ? 1 : e)
        }

        // ── occupancy grid (2 px cells, owner ids) ─────────────────────────
        var C = 2
        var GW = Math.ceil(W / C), GH = Math.ceil(H / C)
        var occ = new Int32Array(GW * GH)
        function diskCells(x, y, r, id, stamp) {
            var i0 = Math.max(0, Math.floor((x - r) / C)), i1 = Math.min(GW - 1, Math.floor((x + r) / C))
            var j0 = Math.max(0, Math.floor((y - r) / C)), j1 = Math.min(GH - 1, Math.floor((y + r) / C))
            var r2 = r * r
            for (var j = j0; j <= j1; j++) {
                var py = (j + 0.5) * C - y
                for (var i = i0; i <= i1; i++) {
                    var px = (i + 0.5) * C - x
                    if (px * px + py * py > r2) continue
                    var k = j * GW + i
                    if (stamp) { if (occ[k] === 0) occ[k] = id }
                    else if (occ[k] !== 0 && occ[k] !== id) return true
                }
            }
            return false
        }
        function segCells(ax, ay, bx, by, r, id, stamp) {
            var L = Math.sqrt((bx - ax) * (bx - ax) + (by - ay) * (by - ay))
            var n = Math.max(1, Math.ceil(L / 1.5))
            for (var t = 0; t <= n; t++) {
                var f = t / n
                if (diskCells(ax + (bx - ax) * f, ay + (by - ay) * f, r, id, stamp) && !stamp) return true
            }
            return false
        }
        function polyHit(p, id) {
            for (var i = 0; i + 3 < p.length; i += 2)
                if (segCells(p[i], p[i + 1], p[i + 2], p[i + 3], TW / 2 + GAP, id, false)) return true
            return false
        }
        function polyStamp(p, id) {
            for (var i = 0; i + 3 < p.length; i += 2)
                segCells(p[i], p[i + 1], p[i + 2], p[i + 3], TW / 2 + 0.5, id, true)
        }
        function polyDensOk(p, reach) {
            for (var i = 0; i + 3 < p.length; i += 2) {
                var ax = p[i], ay = p[i + 1], bx = p[i + 2], by = p[i + 3]
                var L = Math.sqrt((bx - ax) * (bx - ax) + (by - ay) * (by - ay))
                var n = Math.max(1, Math.ceil(L / 12))
                for (var t = 0; t <= n; t++)
                    if (dens(ax + (bx - ax) * t / n, ay + (by - ay) * t / n) < reach) return false
            }
            return true
        }
        function onBoard(x, y, m) { return x >= -m && y >= -m && x <= W + m && y <= H + m }

        var tracks = []   // flat [x0,y0,x1,y1,…]
        var rings = []    // [x, y, r]
        var nextId = 1

        // ── mitred offset of a centreline (dirs[k] runs pts[k]→pts[k+1]) ──
        function offsetPoly(pts, dirs, o) {
            var out = []
            for (var k = 0; k < pts.length; k++) {
                var dIn = k > 0 ? dirs[k - 1] : dirs[0]
                var dOut = k < dirs.length ? dirs[k] : dirs[dirs.length - 1]
                var mx = nx(dIn) + nx(dOut), my = ny(dIn) + ny(dOut)
                var ml = Math.sqrt(mx * mx + my * my)
                mx /= ml; my /= ml
                var f = o / (mx * nx(dIn) + my * ny(dIn))
                out.push(pts[k][0] + mx * f, pts[k][1] + my * f)
            }
            return out
        }

        // ── the walker: grows a centreline for a group of offsets ─────────
        // g: { x, y, dir, offs, id, reach, segs, orth:[a,b], diag:[a,b],
        //      edgeBias, endR, heads: prefix polylines per member (or null) }
        function walk(g) {
            var pts = [[g.x, g.y]], dirs = []
            var d0 = g.dir
            // a bend eats |o|·tan(22.5°) from each side of a segment on the
            // inside member; two bends need 0.83·|o| or the offsets fold
            var maxOff = 0
            for (var mo = 0; mo < g.offs.length; mo++) maxOff = Math.max(maxOff, Math.abs(g.offs[mo]))
            var bendMin = 0.83 * maxOff + 8
            for (var k = 0; k < g.segs; k++) {
                var opts
                if (k === 0) opts = [g.dir]
                else {
                    var pd = dirs[dirs.length - 1]
                    opts = []
                    for (var t = -1; t <= 1; t += 2) {
                        var nd = (pd + t + 8) % 8
                        var sp = (((nd - d0) + 12) % 8) - 4
                        if (Math.abs(sp) <= 2) opts.push(nd)
                    }
                    if ((pd & 1) === 1 && rnd() < 0.15) opts.push(pd)  // occasional long diagonal
                }
                // try the options best-first: stay in the busy margin
                var scored = []
                for (var q = 0; q < opts.length; q++) {
                    var dd = opts[q]
                    var diag = (dd & 1) === 1
                    var L = diag ? rr(g.diag[0], g.diag[1]) : rr(g.orth[0], g.orth[1])
                    var ex = pts[pts.length - 1][0] + DX[dd] * L, ey = pts[pts.length - 1][1] + DY[dd] * L
                    scored.push({ d: dd, L: L, sc: dens(ex, ey) * g.edgeBias + rnd() })
                }
                scored.sort(function (a, b) { return b.sc - a.sc })
                var took = false
                for (q = 0; q < scored.length && !took; q++) {
                    var cand = scored[q]
                    for (var tryN = 0; tryN < 4 && !took; tryN++) {
                        var LL = cand.L * Math.pow(0.62, tryN)
                        var minL = (cand.d & 1) ? g.diag[0] * 0.6 : g.orth[0] * 0.5
                        if (LL < minL || LL < bendMin) break
                        var p = pts[pts.length - 1]
                        var np = [p[0] + DX[cand.d] * LL, p[1] + DY[cand.d] * LL]
                        var tp = pts.concat([np]), td = dirs.concat([cand.d])
                        // extend the test past the end by the worst mitre overhang
                        var ok = true
                        for (var m = 0; m < g.offs.length && ok; m++) {
                            var mp = offsetPoly(tp, td, g.offs[m])
                            var o = Math.abs(g.offs[m]) * 0.42 + 2
                            var n2 = mp.length
                            var seg = [mp[n2 - 4], mp[n2 - 3],
                                       mp[n2 - 2] + DX[cand.d] * o, mp[n2 - 1] + DY[cand.d] * o]
                            if (n2 >= 6) seg = [mp[n2 - 6], mp[n2 - 5]].concat(seg)
                            if (polyHit(seg, g.id) || !polyDensOk(seg, g.reach)) ok = false
                        }
                        if (ok) { pts = tp; dirs = td; took = true }
                    }
                }
                if (!took) break
                var e = pts[pts.length - 1]
                if (!onBoard(e[0], e[1], -2)) break   // left the board: it runs on off-screen
            }
            return dirs.length ? { pts: pts, dirs: dirs } : null
        }

        // commit a group: member polylines + end rings (vias, staggered)
        function commit(g, w, ringsAtStart) {
            var members = []
            for (var m = 0; m < g.offs.length; m++) {
                var body = offsetPoly(w.pts, w.dirs, g.offs[m])
                var head = g.heads ? g.heads[m] : null
                members.push(head ? head.slice(0, head.length - 2).concat(body) : body)
            }
            // end: off the board runs on; on the board ends on staggered vias
            var last = w.pts[w.pts.length - 1], ld = w.dirs[w.dirs.length - 1]
            var endRings = []
            if (onBoard(last[0], last[1], -2)) {
                var stag = g.offs.length > 1 ? 2 * VIA + GAP + 2 : 0
                for (m = 0; m < members.length; m++) {
                    var p = members[m], n = p.length
                    var ext = (m % 2) * stag
                    var ex = p[n - 2] + DX[ld] * ext, ey = p[n - 1] + DY[ld] * ext
                    if (ext > 0) p.push(ex, ey)
                    endRings.push([ex, ey, g.endR])
                }
                for (m = 0; m < endRings.length; m++)
                    if (diskCells(endRings[m][0], endRings[m][1], g.endR + RING / 2 + GAP, g.id, false)) return false
            }
            for (m = 0; m < members.length; m++) {
                if (members[m].length < 4) return false
                if (polyHit(members[m], g.id)) return false
            }
            for (m = 0; m < members.length; m++) { polyStamp(members[m], g.id); tracks.push(members[m]) }
            for (m = 0; m < endRings.length; m++) {
                diskCells(endRings[m][0], endRings[m][1], g.endR + RING / 2 + 0.5, g.id, true)
                rings.push(endRings[m])
            }
            if (ringsAtStart) for (m = 0; m < ringsAtStart.length; m++) rings.push(ringsAtStart[m])
            return true
        }

        function pickSpot(minDens, tries) {
            for (var t = 0; t < tries; t++) {
                var x = rr(0, W), y = rr(0, H)
                var dn = dens(x, y)
                if (dn >= minDens && rnd() < dn * dn) return [x, y]
            }
            return null
        }

        var scale = Math.sqrt(W * H / (1920 * 1080))

        // ── 1. edge buses: enter from beyond the board, run the margin ─────
        var nBus = Math.round(9 * scale)
        for (var b = 0, att = 0; b < nBus && att < nBus * 12; att++) {
            var side = ri(0, 3)   // 0 top, 1 right, 2 bottom, 3 left
            var along = side % 2 === 0 ? rr(0.04, 0.96) * W : rr(0.04, 0.96) * H
            var sx, sy, sd
            if (side === 0) { sx = along; sy = -24; sd = 2 }
            else if (side === 1) { sx = W + 24; sy = along; sd = 4 }
            else if (side === 2) { sx = along; sy = H + 24; sd = 6 }
            else { sx = -24; sy = along; sd = 0 }
            var n = ri(3, 8), pitch = rr(9.5, 11)
            var offs = []
            for (var i = 0; i < n; i++) offs.push((i - (n - 1) / 2) * pitch)
            var g = { x: sx, y: sy, dir: sd, offs: offs, id: nextId++, reach: rr(0.18, 0.5),
                      segs: ri(4, 9), orth: [60, 360], diag: [24, 110], edgeBias: 2.2, endR: VIA }
            var w = walk(g)
            if (!w || w.dirs.length < 2) continue
            if (commit(g, w, null)) b++
        }

        // ── 2. fanouts: a pad row whose tracks converge into a bus ────────
        var nFan = Math.round(16 * scale)
        for (var f = 0, fatt = 0; f < nFan && fatt < nFan * 25; fatt++) {
            var spot = pickSpot(0.35, 60)
            if (!spot) continue
            var d = ri(0, 3) * 2                     // leave the row orthogonally
            var ax = -DY[d], ay = DX[d]              // the row runs across it
            var k = ri(4, 12), pp = rr(15, 18)
            var q = rnd() < 0.7 ? rr(9.5, 10.5) : pp
            var c = (k - 1) / 2, lead = rr(8, 18)
            var maxShift = c * Math.abs(pp - q)
            var conv = lead + maxShift + rr(6, 20)
            var heads = [], starts = [], foffs = []
            var bad = false
            for (i = 0; i < k; i++) {
                var px = spot[0] + (i - c) * pp * ax, py = spot[1] + (i - c) * pp * ay
                if (!onBoard(px, py, -12) || dens(px, py) < 0.2) { bad = true; break }
                var shift = (i - c) * (q - pp)       // along the row axis
                var h = [px, py, px + DX[d] * lead, py + DY[d] * lead]
                var hx = h[2], hy = h[3]
                if (Math.abs(shift) > 0.01) {
                    hx += ax * shift + DX[d] * Math.abs(shift)
                    hy += ay * shift + DY[d] * Math.abs(shift)
                    h.push(hx, hy)
                }
                var tx = spot[0] + (i - c) * q * ax + DX[d] * conv
                var ty = spot[1] + (i - c) * q * ay + DY[d] * conv
                h.push(tx, ty)
                heads.push(h)
                starts.push([px, py, PAD])
                foffs.push((i - c) * q * (ax * nx(d) + ay * ny(d)))
            }
            if (bad) continue
            var fid = nextId++
            var ringBad = false
            for (i = 0; i < starts.length && !ringBad; i++)
                if (diskCells(starts[i][0], starts[i][1], PAD + RING / 2 + GAP, fid, false)) ringBad = true
            if (ringBad) continue
            var hb = false
            for (i = 0; i < heads.length && !hb; i++) if (polyHit(heads[i], fid)) hb = true
            if (hb) continue
            // claim the pads before the walk so the bus cannot fold back over them
            var cxr = spot[0] + DX[d] * conv, cyr = spot[1] + DY[d] * conv
            g = { x: cxr, y: cyr, dir: d, offs: foffs, id: fid, reach: rr(0.12, 0.4),
                  segs: ri(2, 6), orth: [30, 240], diag: [16, 80], edgeBias: 1.6,
                  endR: VIA, heads: heads }
            w = walk(g)
            if (!w) continue
            if (commit(g, w, starts)) {
                for (i = 0; i < starts.length; i++)
                    diskCells(starts[i][0], starts[i][1], PAD + RING / 2 + 0.5, fid, true)
                f++
            }
        }

        // ── 3. singles: short hops from a pad or via to a via ──────────────
        var nSingle = Math.round(240 * scale)
        for (var sgl = 0, satt = 0; sgl < nSingle && satt < nSingle * 6; satt++) {
            var sp2 = pickSpot(0.08, 40)
            if (!sp2) continue
            var sid = nextId++
            var r0 = rnd() < 0.45 ? PAD : VIA
            if (diskCells(sp2[0], sp2[1], r0 + RING / 2 + GAP, sid, false)) continue
            var sg = { x: sp2[0], y: sp2[1], dir: ri(0, 3) * 2 + (rnd() < 0.2 ? 1 : 0), offs: [0], id: sid,
                       reach: rr(0.05, 0.6), segs: ri(1, 5), orth: [30, 180], diag: [12, 60],
                       edgeBias: 0.8, endR: rnd() < 0.8 ? VIA : PAD }
            var sw = walk(sg)
            if (!sw) continue
            var tot = 0
            for (i = 0; i + 1 < sw.pts.length; i++)
                tot += Math.abs(sw.pts[i + 1][0] - sw.pts[i][0]) + Math.abs(sw.pts[i + 1][1] - sw.pts[i][1])
            if (tot < 40) continue
            if (commit(sg, sw, [[sp2[0], sp2[1], r0]])) {
                diskCells(sp2[0], sp2[1], r0 + RING / 2 + 0.5, sid, true)
                sgl++
            }
        }

        return { tracks: tracks, rings: rings, tw: TW, ring: RING }
    }
    // END-PCB-GENERATOR

    Canvas {
        id: canvas
        anchors.fill: parent
        renderTarget: Canvas.Image
        onPaint: {
            var ctx = getContext("2d")
            var bd = root.ensureBoard()
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

    onWidthChanged: canvas.requestPaint()
    onHeightChanged: canvas.requestPaint()
    onKitChanged: canvas.requestPaint()
}
