// Trace.js — cadenza's one pulse engine: the travelling light (intent §2
// "Motion", §3.2 the bar's lamps, §3.9 the cover's board).
//
// Pure JS on purpose (`.pragma library`, no QML, no clock, no Math.random):
// every function here is a total function of its arguments, so the engine is
// the SAME bytes whether it runs under Quickshell or under `node` — that is
// what design/trace.test.js executes, and it is why the board's generator
// keeps its own `node design/...`-free purity too.
//
// WHY THIS FILE EXISTS. The bar grew its own polyline walker inside one
// lamp's inline `segs`/`at` bindings (bar.qml), and the cover needs the same
// walk for every pulse on every track. Two copies of "where is the light now"
// is the thing to avoid, so the walker lives here once and the bar's lamp
// reads it too — see `at`, which is a verbatim port (same operation order,
// same return shape) of the binding it replaced.
//
// THE MODEL — one mechanism, three looks. A pulse is a RECORD, never an
// object per animation: `{ track, pts, total, u, dir, leg, legs, … }`. One
// `advance` steps every record, one painter draws them all. The looks are
// data, not code paths:
//   · a scan to a node and gone      legs = 1
//   · a slide that bounces back       legs = 2..4, velocity easing both ends,
//                                     alpha decaying a step per leg
//   · a node that just breathes      legs = 0 (it sits, `dwell`), no travel
// A pulse always runs ALONG A TRACK and arrives AT A NODE (a board ring) —
// `index()` builds that little graph once, so a pulse only ever names a track
// and a direction.
//
// COORDINATES. A polyline is FLAT — `[x0, y0, x1, y1, …]` — the board's own
// shape. `flat()` adapts the bar lamp's `[{x, y}, …]`.

// ── the walker ──────────────────────────────────────────────────────────────

// [{x, y}, …] → flat. The bar's lamps build point lists; the board is flat.
function flat(pts) {
    var out = []
    for (var i = 0; i < pts.length; i++) out.push(pts[i].x, pts[i].y)
    return out
}

// Segment table of a flat polyline: Manhattan length per segment (the board is
// orthogonal with 45° bends, and the lamp this was lifted from measured it
// that way — the bar's own wire metric, kept).
function segs(pts) {
    var list = [], total = 0
    for (var i = 2; i + 1 < pts.length; i += 2) {
        var x0 = pts[i - 2], y0 = pts[i - 1], x1 = pts[i], y1 = pts[i + 1]
        var len = Math.abs(x1 - x0) + Math.abs(y1 - y0)
        list.push({ x0: x0, y0: y0, x1: x1, y1: y1, s: total, len: len })
        total += len
    }
    return { list: list, total: total }
}

// The point `d` px along a flat polyline, clamped to its ends.
// Return shape is the bar lamp's own `at` (ported verbatim, operation order
// and all) so a caller can clamp its head inside the segment it is on.
function at(pts, d) {
    var S = segs(pts)
    var L = S.list
    var T = S.total
    d = d < 0 ? 0 : (d > T ? T : d)
    for (var i = 0; i < L.length; i++) {
        var g = L[i]
        if (d <= g.s + g.len || i === L.length - 1) {
            var u = g.len > 0 ? Math.min(1, Math.max(0, (d - g.s) / g.len)) : 0
            return { x: g.x0 + (g.x1 - g.x0) * u, y: g.y0 + (g.y1 - g.y0) * u,
                     horiz: g.y0 === g.y1,
                     x0: Math.min(g.x0, g.x1), x1: Math.max(g.x0, g.x1),
                     y0: Math.min(g.y0, g.y1), y1: Math.max(g.y0, g.y1) }
        }
    }
    return { x: 0, y: 0, horiz: true, x0: 0, x1: 0, y0: 0, y1: 0 }
}

function length(pts) { return segs(pts).total }

// ── randomness ──────────────────────────────────────────────────────────────
// mulberry32, the generator's own PRNG (kept local here rather than shared:
// the board's generator must stay liftable as one self-contained block).
function rng(seed) {
    var s = seed >>> 0
    return function () {
        s = (s + 0x6D2B79F5) >>> 0
        var t = s
        t = Math.imul(t ^ (t >>> 15), t | 1)
        t ^= t + Math.imul(t ^ (t >>> 7), t | 61)
        return ((t ^ (t >>> 14)) >>> 0) / 4294967296
    }
}
function rr(r, a, b) { return a + (b - a) * r() }
function ri(r, a, b) { return Math.floor(rr(r, a, b + 1)) }
function pickIdx(r, n) { return n > 0 ? Math.floor(r() * n) % n : -1 }

// A stable 32-bit hash: an agent keeps the same node across reloads (hash of
// its sessionId, never its index in a list that reorders).
function hash(str) {
    var h = 0x811c9dc5
    for (var i = 0; i < str.length; i++) {
        h ^= str.charCodeAt(i)
        h = Math.imul(h, 0x01000193) >>> 0
    }
    return h >>> 0
}

// ── the board's little graph ────────────────────────────────────────────────
// Built once per board. `ends[t]` = the ring each end of track `t` lands on
// (-1 = runs off the board); `near[r]` = every track that ends on ring `r`.
// A pulse therefore names a track and a direction, never coordinates.
function index(board, tol) {
    var TOL = tol || 16
    var tracks = board.tracks, rings = board.rings
    var ends = [], near = []
    for (var r = 0; r < rings.length; r++) near.push([])
    function landing(x, y) {
        var best = -1, bd = TOL * TOL
        for (var k = 0; k < rings.length; k++) {
            var dx = rings[k][0] - x, dy = rings[k][1] - y
            var d = dx * dx + dy * dy
            if (d <= bd) { bd = d; best = k }
        }
        return best
    }
    for (var t = 0; t < tracks.length; t++) {
        var p = tracks[t], n = p.length
        var a = landing(p[0], p[1]), b = landing(p[n - 2], p[n - 1])
        ends.push([a, b])
        if (a >= 0) near[a].push(t)
        if (b >= 0 && b !== a) near[b].push(t)
    }
    var wired = []
    for (t = 0; t < ends.length; t++) if (ends[t][0] >= 0 || ends[t][1] >= 0) wired.push(t)
    return { tol: TOL, ends: ends, near: near, wired: wired }
}

// ── the feed: what the machine is actually doing ────────────────────────────
// sessions.json / hooks.json, parsed once here so both the bar and the cover
// read the same shape (the bar keeps its own FileViews — this is the parse).
function sessions(text) {
    try {
        var d = JSON.parse(text)
        return d && d.sessions && d.sessions.length !== undefined ? d.sessions : []
    } catch (e) { return [] }
}

// `{ sessionId: state }` for sessions that are someone's business: stopped and
// failed are gone (a stopped process lights nothing).
function live(rows) {
    var st = {}
    for (var i = 0; i < rows.length; i++) {
        var s = rows[i]
        if (!s || !s.sessionId) continue
        if (s.state === "stopped" || s.state === "failed") continue
        st[s.sessionId] = "" + (s.state || "idle")
    }
    return st
}

// `{ sessionId: updatedAt }` from hooks.json — the freshness stamp a firing
// agent advances (the bar's `detectActivity`: activity is a stamp ADVANCING
// between two reads, never a level).
function stamps(text) {
    var out = {}
    try {
        var d = JSON.parse(text)
        var a = d && d.hooks && d.hooks.length !== undefined ? d.hooks : []
        for (var i = 0; i < a.length; i++) {
            if (!a[i] || !a[i].sessionId) continue
            out[a[i].sessionId] = Date.parse(a[i].updatedAt || "") || 0
        }
    } catch (e) { }
    return out
}

// Ids whose stamp advanced between two reads (never on a first read).
function advanced(prev, next, first) {
    var fire = []
    if (first) return fire
    for (var k in next) if (prev[k] !== undefined && next[k] > prev[k]) fire.push(k)
    return fire
}

// ── the pulse budget: how much light the board is allowed ───────────────────
// Every live session claims a node (more processes, more nodes). Pulses scale
// with the ones that are ACTING: working and awaiting. The idle floor keeps
// the board alive with nothing running — a tube never fully dark.
function want(states, opts) {
    var o = opts || {}
    var floor = o.idleFloor !== undefined ? o.idleFloor : 2
    var cap = o.cap !== undefined ? o.cap : 14
    var n = floor
    for (var k in states) {
        if (states[k] === "working") n += 2
        else if (states[k] === "awaiting") n += 1
    }
    return Math.min(cap, n)
}

// ── spawn ───────────────────────────────────────────────────────────────────
// One record. `spec` supplies what the caller knows (the track, the direction,
// the node it runs into, the colour its state earns); everything that varies
// per pulse comes from `rng` — speed, tail, legs, dwell — so no two pulses on
// the board ever move alike.
function spawn(r, spec) {
    var pts = spec.pts
    var total = length(pts)
    var legs = spec.legs !== undefined ? spec.legs : (r() < 0.55 ? 1 : ri(r, 2, 4))
    return {
        track: spec.track,
        pts: pts,
        total: total,
        u: 0,
        dir: spec.dir,
        leg: 0,
        legs: legs,
        dwell: legs === 0,
        speed: spec.speed !== undefined ? spec.speed : rr(r, 70, 210),
        trail: spec.trail !== undefined ? spec.trail : rr(r, 14, 46),
        head: spec.head !== undefined ? spec.head : rr(r, 2.2, 3.6),
        hue: spec.hue,
        alpha: spec.alpha !== undefined ? spec.alpha : rr(r, 0.45, 0.9),
        decay: rr(r, 0.35, 0.6),
        state: spec.state || "",
        ring: spec.ring !== undefined ? spec.ring : -1,
        ringOut: spec.ringOut !== undefined ? spec.ringOut : -1,
        bloom: 0,
        bloomRing: -1,
        dying: false,
        age: 0
    }
}

// A dwell (no travel): the node breathes where it is, for `ttl` seconds, then
// fades. It is the third look — a pad that answers without a traveller — and
// it is why a dwell carries a lifetime: without one, dwells would fill the
// budget and no scan could ever spawn again.
function dwell(r, ring, hue, alpha, ttl) {
    return { track: -1, pts: [], total: 0, u: 0, dir: 1, leg: 0, legs: 0,
             dwell: true, speed: 0, trail: 0, head: 0, hue: hue,
             alpha: alpha, decay: 0, state: "", ring: ring, ringOut: -1,
             bloom: 0, bloomRing: -1, ttl: ttl || 6, dying: false, age: 0 }
}

// ── the scheduler: keep the board's light at the level the machine earns ────
// Pure: given what is running (`states`, a `{id: state}` map), the rings it
// has claimed, and the pulses still alive, hand back the array to draw next.
// Spawns are random — WHICH agent, WHICH track, HOW fast — because a board
// where everything moves at once reads as an effect, not as a machine.
function reconcile(list, states, ctx) {
    var r = ctx.rng, board = ctx.board, ix = ctx.index, hues = ctx.hues
    var alive = []
    for (var i = 0; i < list.length; i++) if (!list[i].dying) alive.push(list[i])
    var counts = {}
    for (i = 0; i < alive.length; i++) if (alive[i].ring >= 0) counts[alive[i].ring] = (counts[alive[i].ring] || 0) + 1

    var n = want(states, ctx.opts)
    // Over budget: the least interesting pulses die, youngest first.
    if (alive.length > n) alive.length = n
    // Under budget: spawn toward the agents, idle light when nobody acts.
    var ids = Object.keys(states)
    var guard = 0
    while (alive.length < n && guard++ < 40) {
        var ring = -1, state = ""
        for (var t = 0; t < ids.length && ring < 0; t++) {
            var id = ids[pickIdx(r, ids.length)]
            var cand = ix.near.length ? hash(id) % ix.near.length : -1
            if (cand >= 0 && ix.near[cand].length > 0 && (counts[cand] || 0) < 2) {
                ring = cand
                state = states[id]
            }
        }
        // No agent to run to (or they are all busy): the idle roam — a random
        // wired track, dim, slower, arriving at whichever node it ends on. On a
        // board with NOTHING running, one in six of those is instead a NODE
        // that simply breathes: an empty board should still look inhabited,
        // not swept. With agents up, the light is always a traveller.
        if (ring < 0) {
            if (!ix.wired.length) break
            var track = ix.wired[pickIdx(r, ix.wired.length)]
            var e = ix.ends[track]
            var toRing = e[1] >= 0 ? e[1] : e[0]
            if (ids.length === 0 && toRing >= 0 && r() < 0.18) {
                alive.push(dwell(r, toRing, hues.idle, rr(r, 0.18, 0.35), rr(r, 3, 9)))
                continue
            }
            alive.push(spawn(r, {
                track: track, pts: board.tracks[track],
                dir: e[1] >= 0 ? 1 : -1,
                ring: toRing, ringOut: e[0] >= 0 ? e[0] : -1,
                legs: r() < 0.7 ? 1 : ri(r, 2, 3),
                speed: rr(r, 40, 95), trail: rr(r, 20, 60),
                hue: hues.idle, alpha: rr(r, 0.18, 0.4), head: rr(r, 1.8, 2.6)
            }))
            continue
        }
        var near = ix.near[ring]
        var tr = near[pickIdx(r, near.length)]
        var ends = ix.ends[tr]
        var into = ends[1] === ring ? 1 : -1
        counts[ring] = (counts[ring] || 0) + 1
        var busy = state === "working"
        alive.push(spawn(r, {
            track: tr, pts: board.tracks[tr], dir: into,
            ring: ring, ringOut: ends[0] === ring ? ends[1] : ends[0],
            legs: busy ? ri(r, 2, 4) : ri(r, 1, 3),
            speed: busy ? rr(r, 120, 260) : rr(r, 50, 120),
            trail: busy ? rr(r, 24, 60) : rr(r, 16, 40),
            hue: busy ? hues.working : hues.awaiting,
            alpha: busy ? rr(r, 0.6, 1.0) : rr(r, 0.4, 0.75),
            state: state, head: busy ? rr(r, 2.6, 4.0) : rr(r, 2.2, 3.2)
        }))
    }
    return alive
}

// ── the step ────────────────────────────────────────────────────────────────
// dt in seconds. `u` is normalised progress along the CURRENT leg, so a leg's
// duration is independent of the easing below: the light looks like it
// settles, and the timing stays honest.
function advance(list, dt) {
    var out = []
    for (var i = 0; i < list.length; i++) {
        var p = list[i]
        p.age += dt
        // Death is checked BEFORE the look it is dying of: a dwell that dies
        // must reach its fade, and a dwell branch that ran first would hold it
        // breathing forever (the bug this ordering exists to prevent).
        if (p.dying) {
            p.alpha -= dt * 1.8
            p.bloom = Math.max(0, p.bloom - dt * 1.4)
            if (p.alpha > 0.02) out.push(p)
            continue
        }
        if (p.dwell) {
            // breathe: up and down, for its lifetime, then die into the fade
            p.bloom = 0.5 + 0.5 * Math.sin(p.age * 1.6 + p.alpha * 9)
            if (p.ttl > 0) { p.ttl -= dt; if (p.ttl <= 0) p.dying = true }
            out.push(p)
            continue
        }
        p.u += (p.speed / Math.max(1, p.total)) * dt
        if (p.bloom > 0) p.bloom = Math.max(0, p.bloom - dt * 2.2)
        if (p.u >= 1) {
            // arrival: the node it ran into blinks, then the light either
            // turns around (a bounce, dimmer) or dies.
            p.bloom = 1
            p.bloomRing = p.dir > 0 ? p.ring : p.ringOut
            p.leg++
            p.alpha *= (1 - p.decay)
            p.dir = -p.dir
            p.u = 0
            if (p.leg >= p.legs || p.alpha <= 0.08) p.dying = true
        }
        out.push(p)
    }
    return out
}

// ── presentation (pure, so the painter holds no maths) ──────────────────────
// Velocity: slow out of a node, slow into the next — the light slides rather
// than travels, which is what a signal on copper looks like.
function ease(u) { return u * u * (3 - 2 * u) }

// Where the head is, in px along the track.
function dist(p) {
    if (p.dwell) return 0
    var e = ease(p.u)
    return p.dir > 0 ? e * p.total : (1 - e) * p.total
}

// A point `back` px BEHIND the head (the tail), clamped at the track's end.
function tail(p, back) { return at(p.pts, dist(p) - p.dir * back) }

// The head itself (the tail at zero back, named so a caller does not have to
// write `tail(p, 0)` and mean the front).
function head(p) { return at(p.pts, dist(p)) }
