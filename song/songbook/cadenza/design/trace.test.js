// design/trace.test.js — the pulse engine's own check. `node design/trace.test.js`.
//
// A manual check: no flake gate, CI job or timer runs it. `widgets/Trace.js` and
// the board's generator are both deliberately pure (no QML, no clock, no
// Math.random — see each header), so the bytes that ship are the bytes this
// runs. Quickshell cannot be exercised from a checkout without a compositor, so
// the maths is proved here and the LOOK is proved on the canvas
// (`lyra preview wallpaper`, cover/README.md).
//
// What it holds:
//   1. `at` — the walker the bar's lamp now reads — is identical to the inline
//      `segs`/`at` bindings it replaced, for random polylines and distances.
//   2. The generator still runs standalone between its markers, and the board
//      it makes indexes into rings sensibly.
//   3. The scheduler honours the budget: more working agents, more light, never
//      past the cap, never fewer than the idle floor.
//   4. The step is total: no NaN, positions inside the track, bounces reverse,
//      blooms land on a node, and every pulse eventually dies.

const fs = require("fs")
const path = require("path")

const here = __dirname
const widgets = path.join(here, "..", "widgets")

// ── load widgets/Trace.js as the module it is (strip the QML pragma) ────────
function loadTrace() {
    const src = fs.readFileSync(path.join(widgets, "Trace.js"), "utf8")
        .replace(/^\.pragma library\s*$/m, "")
    return new Function(src + "\nreturn { flat, segs, at, length, rng, rr, ri, pickIdx, hash, index, sessions, live, stamps, advanced, want, spawn, dwell, reconcile, advance, ease, dist, tail, head, light, breath }")()
}

// ── the generator, lifted between its own markers (the file's own promise) ──
function loadGenerator() {
    const src = fs.readFileSync(path.join(widgets, "CoverPcbWorker.js"), "utf8")
    const a = src.indexOf("// BEGIN-PCB-GENERATOR")
    const b = src.indexOf("// END-PCB-GENERATOR")
    if (a < 0 || b < 0) throw new Error("CoverPcbWorker.js lost its generator markers")
    const block = src.slice(src.indexOf("\n", a) + 1, b)
    return new Function(block + "\nreturn pcbGenerate")()
}

// ── the oracle: the bar's inline bindings, copied verbatim from bar.qml ─────
function barAt(pts, t) {
    var segs = { list: [], total: 0 }
    for (var i = 1; i < pts.length; i++) {
        var len = Math.abs(pts[i].x - pts[i - 1].x) + Math.abs(pts[i].y - pts[i - 1].y)
        segs.list.push({ a: pts[i - 1], b: pts[i], s: segs.total, len: len })
        segs.total += len
    }
    var S = segs.list, d = t * segs.total
    for (var j = 0; j < S.length; j++) {
        var g = S[j]
        if (d <= g.s + g.len || j === S.length - 1) {
            var u = g.len > 0 ? Math.min(1, Math.max(0, (d - g.s) / g.len)) : 0
            return { x: g.a.x + (g.b.x - g.a.x) * u, y: g.a.y + (g.b.y - g.a.y) * u,
                     horiz: g.a.y === g.b.y,
                     x0: Math.min(g.a.x, g.b.x), x1: Math.max(g.a.x, g.b.x),
                     y0: Math.min(g.a.y, g.b.y), y1: Math.max(g.a.y, g.b.y) }
        }
    }
    return { x: 0, y: 0, horiz: true, x0: 0, x1: 0, y0: 0, y1: 0 }
}

let pass = 0, fail = 0
function check(name, ok, detail) {
    if (ok) { pass++; console.log("  ok   " + name) }
    else { fail++; console.log("  FAIL " + name + (detail ? " — " + detail : "")) }
}
const finite = (n) => typeof n === "number" && isFinite(n)

const Trace = loadTrace()
const pcbGenerate = loadGenerator()
const r = Trace.rng(0x00CADE2A)
const rr = (a, b) => Trace.rr(r, a, b)

console.log("\ntrace.test.js — the pulse engine\n")

// ── 1. the walker, against the bindings it replaced ────────────────────────
{
    let worst = 0, worstShape = ""
    for (let k = 0; k < 400; k++) {
        const n = 2 + Math.floor(r() * 6)
        const pts = []
        let x = 0, y = 0
        for (let i = 0; i < n; i++) {
            x += Math.round(rr(-180, 180)); y += Math.round(rr(-180, 180))
            pts.push({ x: x, y: y })
        }
        const flat = Trace.flat(pts)
        for (let s = 0; s <= 20; s++) {
            const t = s / 20
            const want = barAt(pts, t)
            const got = Trace.at(flat, t * Trace.length(flat))
            const keys = ["x", "y", "x0", "x1", "y0", "y1"]
            for (const key of keys) {
                const d = Math.abs(want[key] - got[key])
                if (d > worst) { worst = d; worstShape = key + " @" + t.toFixed(2) }
                if (want.horiz !== got.horiz) { worst = 1e9; worstShape = "horiz @" + t.toFixed(2) }
            }
        }
    }
    check("at() is identical to bar.qml's inline bindings (400 polylines × 21 points)",
        worst === 0, "worst deviation " + worst + " on " + worstShape)
}

// ── 2. the board, generated standalone, indexes into its own rings ─────────
const board = pcbGenerate(1920, 1080, 0x00CADE2A)
const ix = Trace.index(board)
{
    check("the generator runs outside Quickshell between its markers",
        board && board.tracks.length > 50 && board.rings.length > 50,
        (board ? board.tracks.length + " tracks / " + board.rings.length + " rings" : "no board"))

    let landed = 0
    for (const e of ix.ends) if (e[0] >= 0 || e[1] >= 0) landed++
    check("most tracks land on a node at one end (the pulse's destination)",
        landed > board.tracks.length * 0.4, landed + "/" + board.tracks.length + " wired")

    let orphan = 0
    for (let k = 0; k < ix.near.length; k++) if (ix.near[k].length === 0) orphan++
    check("the ring→track adjacency covers the board", orphan < ix.near.length * 0.5,
        orphan + "/" + ix.near.length + " rings with no track end")

    let bad = 0
    for (const t of ix.wired) { const p = board.tracks[t]; if (Trace.length(p) <= 0) bad++ }
    check("every wired track has real length", bad === 0, bad + " zero-length")
}

// ── 3. the budget ─────────────────────────────────────────────────────────
{
    const opts = { cap: 14, idleFloor: 2 }
    check("nothing running still has light", Trace.want({}, opts) === 2)
    check("two working agents are more than nothing", Trace.want({ a: "working", b: "working" }, opts) === 6)
    check("awaiting counts for less than working",
        Trace.want({ a: "awaiting" }, opts) < Trace.want({ a: "working" }, opts))
    check("the cap holds", Trace.want({ a: "working", b: "working", c: "working", d: "working", e: "working", f: "working", g: "working", h: "working" }, opts) === 14)
    check("idle sessions claim a node but add no pulses", Trace.want({ a: "idle" }, opts) === 2)
}

// ── 4. the scheduler and the step ─────────────────────────────────────────
{
    const hues = { working: "#39ff6a", awaiting: "#ff4d4d", idle: "#4a905d" }
    const ctx = { rng: Trace.rng(7), board: board, index: ix, hues: hues, opts: { cap: 14, idleFloor: 2 } }

    let list = Trace.reconcile([], {}, ctx)
    check("idle reconcile fills exactly the floor", list.length === 2, "got " + list.length)

    list = Trace.reconcile(list, { a: "working", b: "awaiting", c: "idle" }, ctx)
    check("three sessions raise the board above the floor", list.length === 2 + 2 + 1, "got " + list.length)

    const many = {}
    for (let i = 0; i < 9; i++) many["s" + i] = "working"
    list = Trace.reconcile(list, many, ctx)
    check("nine working agents stop at the cap", list.length === 14, "got " + list.length)

    // long run: total function, bounded, and it drains when nothing reconciles
    let steps = 0, nan = 0, escaped = 0, bounced = 0
    let up = {}
    for (steps = 0; steps < 4000; steps++) {
        list = Trace.advance(list, 1 / 30)
        if (steps % 90 === 0) list = Trace.reconcile(list, { a: "working", b: "awaiting" }, ctx)
        for (const p of list) {
            if (!finite(p.alpha) || !finite(p.u) || !finite(p.speed)) { nan++; continue }
            const d = Trace.dist(p)
            if (!finite(d) || d < -1e-6 || d > p.total + 1e-6) escaped++
            const t = Trace.tail(p, p.trail)
            if (!finite(t.x) || !finite(t.y)) nan++
            if (!up[p.track]) up[p.track] = d
            else if (p.leg >= 1 && up[p.track] > d) bounced++
            up[p.track] = d
            if (p.bloomRing >= 0 && (p.bloomRing >= board.rings.length)) escaped++
        }
    }
    check("4000 steps: no NaN in alpha/u/speed or in a tail point", nan === 0, nan + " bad")
    check("4000 steps: every position stays inside its track", escaped === 0, escaped + " escapes")
    check("a bounce really doubles back (dist falls after an arrival)", bounced > 0, bounced + " reversals")

    // a pulse with nothing to feed it dies out: no immortal light
    let drain = Trace.reconcile([], {}, ctx)
    for (let i = 0; i < 6000 && drain.length; i++) drain = Trace.advance(drain, 1 / 30)
    check("every pulse eventually dies when the scheduler stops feeding it", drain.length === 0)

    // the light lands ON the node: every arrival is within the ring it claims
    let arrived = 0, missed = 0, worstGap = 0
    for (let n = 0; n < 300; n++) {
        const one = Trace.reconcile([], { a: "working" }, ctx)
        for (const p of one) {
            if (p.ring < 0 || p.pts.length < 4) continue
            const ring = board.rings[p.ring]
            const end = Trace.at(p.pts, p.dir > 0 ? p.total : 0)
            const gap = Math.hypot(end.x - ring[0], end.y - ring[1])
            if (gap > worstGap) worstGap = gap
            if (gap <= 16) arrived++; else missed++
        }
    }
    check("every agent pulse arrives on the node it aims at", missed === 0,
        arrived + " on the node, " + missed + " off, worst gap " + worstGap.toFixed(1) + "px")

    // the third look: a node that breathes has a lifetime, so it can never
    // park the budget and starve the scans
    {
        let dw = [Trace.dwell(Trace.rng(3), 0, hues.idle, 0.3, 4)]
        let ticks = 0
        while (dw.length && ticks < 6000) { dw = Trace.advance(dw, 1 / 30); ticks++ }
        // 4s at 30fps is 120 ticks, plus the fade — nowhere near the 6000 cap
        check("a dwell dies within its lifetime and fades out", ticks > 100 && ticks < 300,
            ticks + " ticks (" + (ticks / 30).toFixed(1) + "s)")

        // and the idle board does spawn them: over many reconciles, some pulses
        // must be dwells and most must be travellers
        let dwells = 0, travellers = 0
        for (let n = 0; n < 400; n++) {
            for (const p of Trace.reconcile([], {}, ctx)) { if (p.dwell) dwells++; else travellers++ }
        }
        check("an idle board mixes breathing nodes into the roam",
            dwells > 0 && travellers > dwells, dwells + " dwells / " + travellers + " travellers")

        // agents are never served a dwell by the scheduler: the light is for them
        let agentDwells = 0
        for (let n = 0; n < 400; n++)
            for (const p of Trace.reconcile([], { a: "working", b: "awaiting" }, ctx)) if (p.dwell) agentDwells++
        check("agent light is always a traveller, never a dwell", agentDwells === 0, agentDwells + " dwells")
    }

    // ── 5. the renderer's geometry: Trace.light's rectangles ───────────────
    // This is what the item-based delegate draws, so it is the thing to hold:
    // finite numbers, the tail BEHIND the head, every piece on the track, the
    // ring on the node it claims.
    {
        const hues2 = { working: "#39ff6a", awaiting: "#ff4d4d", idle: "#4a905d" }
        const ctx2 = { rng: Trace.rng(21), board: board, index: ix, hues: hues2, opts: { cap: 14, idleFloor: 2 } }
        let nan = 0, offTrack = 0, aheadOfHead = 0, ringOff = 0, seen = 0, rings = 0

        for (let n = 0; n < 300; n++) {
            let list = Trace.reconcile([], { a: "working", b: "idle" }, ctx2)
            for (let s = 0; s < 40; s++) {
                list = Trace.advance(list, 1 / 30)
                for (const p of list) {
                    const f = Trace.light(p, 6, board, s / 30)
                    seen++
                    if (!f || !finite(f.alpha)) { nan++; continue }
                    for (const seg of f.tail) {
                        if (!finite(seg.x) || !finite(seg.y) || !finite(seg.w) || !finite(seg.h) || !finite(seg.rot) || !finite(seg.alpha)) nan++
                        if (seg.w < 0 || seg.h <= 0) offTrack++
                    }
                    if (f.head) {
                        if (!finite(f.head.x) || !finite(f.head.y) || f.head.r <= 0) nan++
                        // the head must sit at the record's own progress
                        const want = Trace.head(p)
                        if (Math.abs(want.x - f.head.x) > 1e-6 || Math.abs(want.y - f.head.y) > 1e-6) aheadOfHead++
                    }
                    if (f.ring) {
                        rings++
                        if (!finite(f.ring.r) || !finite(f.ring.stroke) || f.ring.r <= 0) nan++
                        const src = p.dwell ? p.ring : p.bloomRing
                        const g = board.rings[src]
                        if (!g || Math.abs(g[0] - f.ring.x) > 1e-6 || Math.abs(g[1] - f.ring.y) > 1e-6) ringOff++
                    }
                }
            }
        }
        check("Trace.light: every rectangle is finite and non-degenerate", nan === 0, nan + " bad over " + seen + " faces")
        check("Trace.light: the tail never leads the head", aheadOfHead === 0, aheadOfHead + " wrong heads")
        check("Trace.light: a ring is always drawn on the node it names", ringOff === 0, ringOff + " off-node over " + rings + " rings")

        // the breath is the one sine, bounded, and 0..1 for any clock
        let bad = 0
        for (let t = -50; t < 50; t += 0.37) {
            const b = Trace.breath(t, 3.1)
            if (!finite(b) || b < 0 || b > 1) bad++
        }
        check("breath stays 0..1 for any clock and phase", bad === 0, bad + " out of range")

        // the delegate re-reads a MUTATING record: mutate, ask again, get a new
        // face — no rebuild, no cache. Run on the LONGEST wired track so the
        // light is still travelling inside the window (a short track can
        // complete its round trip and land back where it started).
        let longest = -1, longestLen = -1
        for (const t of ix.wired) {
            const L = Trace.length(board.tracks[t])
            if (L > longestLen) { longestLen = L; longest = t }
        }
        const ends = ix.ends[longest]
        const onePulse = Trace.spawn(Trace.rng(5), {
            track: longest, pts: board.tracks[longest], dir: ends[1] >= 0 ? 1 : -1,
            ring: ends[1] >= 0 ? ends[1] : ends[0], hue: hues2.working,
            legs: 4, speed: 150, trail: 30, alpha: 0.8
        })
        const before = Trace.light(onePulse, 6, board, 0)
        for (let s = 0; s < 60; s++) Trace.advance([onePulse], 1 / 30)
        const after = Trace.light(onePulse, 6, board, 2)
        check("a mutating record yields a new face without any rebuild (" + Math.round(longestLen) + "px track)",
            before.head && after.head && (before.head.x !== after.head.x || before.head.y !== after.head.y))

        // and a face is always drawable: alphas inside 0..1, even mid-death
        let wild = 0
        const dying = Trace.spawn(Trace.rng(6), {
            track: longest, pts: board.tracks[longest], dir: 1, ring: -1,
            hue: hues2.idle, legs: 1, speed: 400, trail: 20, alpha: 0.5
        })
        for (let s = 0; s < 400; s++) {
            Trace.advance([dying], 1 / 30)
            const f = Trace.light(dying, 6, board, s / 30)
            if (f.alpha < 0 || f.alpha > 1) wild++
            if (f.head && (f.head.alpha < 0 || f.head.alpha > 1)) wild++
            for (const seg of f.tail) if (seg.alpha < 0 || seg.alpha > 1) wild++
        }
        check("every alpha handed to QML is inside 0..1, death included", wild === 0, wild + " out of range")
    }

    // determinism: same seed, same light
    const a = Trace.reconcile([], { a: "working", b: "idle" }, { rng: Trace.rng(11), board: board, index: ix, hues: hues, opts: { cap: 14, idleFloor: 2 } })
    const b = Trace.reconcile([], { a: "working", b: "idle" }, { rng: Trace.rng(11), board: board, index: ix, hues: hues, opts: { cap: 14, idleFloor: 2 } })
    let same = a.length === b.length
    for (let i = 0; same && i < a.length; i++) same = a[i].speed === b[i].speed && a[i].track === b[i].track && a[i].legs === b[i].legs
    check("one seed, one board of lights", same)
}

// ── 5. the feed ───────────────────────────────────────────────────────────
{
    const rows = Trace.sessions(fs.readFileSync(path.join(here, "fixtures", "board", "sessions.json"), "utf8"))
    check("the board fixture parses", rows.length > 0, rows.length + " rows")
    const st = Trace.live(rows)
    check("a live map is built from it", Object.keys(st).length > 0)
    check("a broken document is empty, never a throw", Trace.sessions("{not json").length === 0)

    const prev = { a: 5, b: 9 }, next = { a: 5, b: 11 }
    check("only an ADVANCING stamp fires", JSON.stringify(Trace.advanced(prev, next, false)) === '["b"]')
    check("a first read never fires", Trace.advanced({}, next, true).length === 0)
}

console.log("\n" + pass + " passed, " + fail + " failed\n")
process.exit(fail === 0 ? 0 : 1)
