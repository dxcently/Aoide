// BarPatch.qml — the patch panel at the bar's right end (intent §3.2a).
//
// A HELPER (uppercase — never a slot), loaded by URL from bar.qml, which only
// places it. A small live map of the agents, ~16 cells wide, the bar's full
// height, drawn as pixel art like the switchboard's schematic:
//
//      ●   ○   ●───────┐          upper row: roots (a state lamp each)
//      │       │ ● ●   │          2px ink wires: a child hangs off its parent
//      ●       ●─●─●  +3          lower row: spawned children (+ their own
//                                  children, wired along the row's under-bus)
//
//   dots      a state lamp per live agent session (not shells, not ended):
//             working ● · awaiting ◐ · idle ○ · stopped ■ · failed ✕, in the
//             kit's lampColor (intent §2 Instruments)
//   rows      a root on the upper row; its spawned descendants on the lower
//             row, the first directly under it (`graph.json` `spawned` edges,
//             only between two live agents — a shell parent makes a root)
//   wires     a child: up into a comb under its root; a grandchild: along
//             an under-bus below the lower row into its own parent. Junction
//             dots where three wire ends meet (the schematic convention)
//   lamps     activity(ids) — the bar calls it when a session's `hooks.json`
//             `updatedAt` ADVANCES (never on the first read). A child: a
//             short amber dash runs its wire child → parent. A root: its dot
//             flashes amber once. ≤ 6 in flight; a second advance on a wire
//             already running coalesces into it; past the cap it is dropped
//   overflow  whole root columns that do not fit give way to `+n` (cyan)
//   click     activated() → the bar opens the board on OVERVIEW
//   hover     hoveredChanged → the bar hangs `paneComponent` (a kit Pane
//             listing the agents with state) under the panel
// At rest nothing moves: no timer runs, the Canvas repaints only when the
// map or the palette changes.
//
// Reads only what the bar hands it (rule 7): the bar's merged sessions
// (sessions.json + hooks.json phase) and graph.json's `spawned` edges.
import QtQuick
import "Kit.js" as Kit

Item {
    id: patch

    required property var kit
    property var sessions: []           // the bar's merged rows {id, state, kind, name, agent, startedAt}
    property var graphDoc: null         // graph.json (nodes/edges)
    property int lampMs: 600
    property int cells: 16

    signal activated()
    readonly property bool hovered: patchMa.containsMouse

    implicitWidth: kit.cells(cells)
    implicitHeight: 28
    width: implicitWidth
    height: implicitHeight

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

    // ── geometry (px; a wire is its top-left pixel, 2px thick) ───────────────
    //   upper dots 3..8 · parent drop 9..11 · comb 12..13 · child rise 14..16 ·
    //   lower dots 17..22 · under-bus drop 23 · under-bus 24..25 · (trunk 27)
    readonly property int pitch: 12
    readonly property int dotS: 6
    readonly property int upperY: 3
    readonly property int lowerY: 17
    readonly property int combY: 12
    readonly property int underY: 24
    readonly property int wireW: 2
    readonly property int maxLamps: 6
    readonly property int slots: Math.max(1, Math.floor(width / pitch))

    // ── the map ──────────────────────────────────────────────────────────────
    function isLive(s) {
        return !!s && (s.state === "working" || s.state === "awaiting"
                       || s.state === "stopped" || s.state === "idle")
    }
    function isShell(s) {
        return s.kind === "shell" || (s.kind === "" && s.agent === "shell")
    }
    function sid(ref) {
        var v = "" + (ref || "")
        return v.indexOf("session:") === 0 ? v.slice(8) : ""
    }
    readonly property var model: {
        var S = patch.sessions || []
        var live = {}, order = []
        for (var i = 0; i < S.length; i++) {
            var s = S[i]
            if (!s || !s.id || patch.isShell(s) || !patch.isLive(s)) continue
            live[s.id] = s
            order.push(s)
        }
        order.sort(function (a, b) {
            var ta = Date.parse(a.startedAt || "") || 0, tb = Date.parse(b.startedAt || "") || 0
            return ta !== tb ? ta - tb : (a.id < b.id ? -1 : 1)
        })
        // parents from graph.json `spawned` edges, both ends live agents
        var parentOf = {}
        var E = (patch.graphDoc && Array.isArray(patch.graphDoc.edges)) ? patch.graphDoc.edges : []
        for (var e = 0; e < E.length; e++) {
            var g = E[e]
            if (!g || g.kind !== "spawned") continue
            var p = patch.sid(g.from), c = patch.sid(g.to)
            if (p && c && p !== c && live[p] && live[c] && !parentOf[c]) parentOf[c] = p
        }
        // break any cycle: a node whose ancestry loops back is a root
        for (var id in parentOf) {
            var seen = {}, cur = id
            while (parentOf[cur] && !seen[cur]) { seen[cur] = true; cur = parentOf[cur] }
            if (seen[cur]) delete parentOf[id]
        }
        var kids = {}
        for (var q = 0; q < order.length; q++) {
            var pp = parentOf[order[q].id]
            if (pp) { if (!kids[pp]) kids[pp] = []; kids[pp].push(order[q]) }
        }
        // columns: a root, then its descendants depth-first on the lower row
        var cols = []
        function walk(n, depth, list) {
            var k = kids[n.id] || []
            for (var j = 0; j < k.length; j++) {
                list.push({ s: k[j], depth: depth, parent: n.id })
                walk(k[j], depth + 1, list)
            }
        }
        for (var r = 0; r < order.length; r++) {
            if (parentOf[order[r].id]) continue
            var desc = []
            walk(order[r], 1, desc)
            cols.push({ root: order[r], desc: desc, w: Math.max(1, desc.length) })
        }
        // the pane's list: every agent, tree order (drawn or not)
        var list = [], total = 0, working = 0
        for (var c0 = 0; c0 < cols.length; c0++) {
            list.push({ s: cols[c0].root, depth: 0 })
            for (var d0 = 0; d0 < cols[c0].desc.length; d0++) list.push(cols[c0].desc[d0])
        }
        for (var l0 = 0; l0 < list.length; l0++) { total++; if (list[l0].s.state === "working") working++ }
        // fit whole columns; on overflow the last 3 cells hold `+n`
        var need = 0
        for (var c1 = 0; c1 < cols.length; c1++) need += cols[c1].w
        var avail = patch.slots
        if (need > avail) avail = Math.max(0, Math.floor((patch.width - patch.kit.cells(3)) / patch.pitch))
        var dots = [], runs = [], junctions = [], paths = {}, at = {}, slot = 0, shown = 0
        function cx(sl) { return Math.round(patch.pitch / 2 + sl * patch.pitch) }
        for (var c2 = 0; c2 < cols.length; c2++) {
            var C = cols[c2]
            if (slot + C.w > avail) break
            var rx = cx(slot)
            dots.push({ id: C.root.id, x: rx, row: 0, state: C.root.state })
            at[C.root.id] = { x: rx, row: 0 }
            shown++
            // lower row, in walk order
            for (var d1 = 0; d1 < C.desc.length; d1++) {
                var D = C.desc[d1]
                var dx = cx(slot + d1)
                dots.push({ id: D.s.id, x: dx, row: 1, state: D.s.state })
                at[D.s.id] = { x: dx, row: 1, parent: D.parent, depth: D.depth }
                shown++
            }
            // wires: the root's direct children hang off a comb under it
            var direct = C.desc.filter(function (z) { return z.depth === 1 })
            if (direct.length > 0) {
                var lastX = at[direct[direct.length - 1].s.id].x
                runs.push({ x: rx - 1, y: patch.upperY + patch.dotS, w: 2, h: patch.combY - patch.upperY - patch.dotS })
                if (lastX > rx) {
                    runs.push({ x: rx - 1, y: patch.combY, w: lastX - rx + 2, h: 2 })
                    junctions.push({ x: rx, y: patch.combY + 1 })
                } else {
                    runs.push({ x: rx - 1, y: patch.combY, w: 2, h: 2 })
                }
                for (var d2 = 0; d2 < direct.length; d2++) {
                    var kx = at[direct[d2].s.id].x
                    runs.push({ x: kx - 1, y: patch.combY + 2, w: 2, h: patch.lowerY - patch.combY - 2 })
                    if (kx > rx && kx < lastX) junctions.push({ x: kx, y: patch.combY + 1 })
                    paths[direct[d2].s.id] = [{ x: kx - 1, y: patch.lowerY - 1 }, { x: kx - 1, y: patch.combY },
                                              { x: rx - 1, y: patch.combY }, { x: rx - 1, y: patch.upperY + patch.dotS }]
                }
            }
            // deeper: along the under-bus into their own (lower-row) parent
            var byParent = {}
            for (var d3 = 0; d3 < C.desc.length; d3++) {
                var G = C.desc[d3]
                if (G.depth < 2) continue
                if (!byParent[G.parent]) byParent[G.parent] = []
                byParent[G.parent].push(G)
            }
            for (var pid in byParent) {
                var px = at[pid].x, gs = byParent[pid]
                var xs = gs.map(function (z) { return at[z.s.id].x })
                var hiX = Math.max.apply(null, xs.concat([px])), loX = Math.min.apply(null, xs.concat([px]))
                var busTop = patch.lowerY + patch.dotS
                runs.push({ x: px - 1, y: busTop, w: 2, h: patch.underY - busTop })
                runs.push({ x: loX - 1, y: patch.underY, w: hiX - loX + 2, h: 2 })
                for (var g2 = 0; g2 < gs.length; g2++) {
                    var gx = at[gs[g2].s.id].x
                    runs.push({ x: gx - 1, y: busTop, w: 2, h: patch.underY - busTop })
                    if (gx > loX && gx < hiX) junctions.push({ x: gx, y: patch.underY + 1 })
                    paths[gs[g2].s.id] = [{ x: gx - 1, y: busTop }, { x: gx - 1, y: patch.underY },
                                          { x: px - 1, y: patch.underY }, { x: px - 1, y: busTop }]
                }
            }
            slot += C.w
        }
        return { dots: dots, runs: runs, junctions: junctions, paths: paths, at: at,
                 hidden: total - shown, list: list, total: total, working: working }
    }

    // ── paint ────────────────────────────────────────────────────────────────
    Canvas {
        id: art
        anchors.fill: parent
        readonly property var paintKey: [patch.model, patch.kit.ink, patch.kit.title, patch.kit.urgent, patch.kit.mid]
        onPaintKeyChanged: requestPaint()
        Component.onCompleted: requestPaint()
        onPaint: {
            var ctx = getContext("2d")
            ctx.reset()
            ctx.clearRect(0, 0, width, height)
            var M = patch.model, n = patch.dotS
            ctx.fillStyle = "" + patch.kit.ink
            for (var i = 0; i < M.runs.length; i++) {
                var w = M.runs[i]
                if (w.w > 0 && w.h > 0) ctx.fillRect(w.x, w.y, w.w, w.h)
            }
            for (var j = 0; j < M.junctions.length; j++) {
                var c = M.junctions[j]
                ctx.fillRect(c.x - 2, c.y - 1, 4, 2)
                ctx.fillRect(c.x - 1, c.y - 2, 2, 4)
            }
            // state lamps, 6px, drawn as pixel shapes of the kit's glyphs
            for (var k = 0; k < M.dots.length; k++) {
                var d = M.dots[k]
                var l = d.x - n / 2, t = d.row === 0 ? patch.upperY : patch.lowerY
                ctx.clearRect(l, t, n, n)
                ctx.fillStyle = "" + patch.kit.lampColor(d.state)
                switch (d.state) {
                case "working":                              // ● filled, corners cut
                    ctx.fillRect(l + 1, t, n - 2, n); ctx.fillRect(l, t + 1, n, n - 2); break
                case "stopped":                              // ■ filled square
                    ctx.fillRect(l, t, n, n); break
                case "failed":                               // ✕ two diagonals
                    for (var p = 0; p < n; p++) { ctx.fillRect(l + p, t + p, 1, 1); ctx.fillRect(l + n - 1 - p, t + p, 1, 1) }
                    break
                case "awaiting":                             // ◐ ring, left half filled
                    ctx.fillRect(l + 1, t, n - 2, 1); ctx.fillRect(l + 1, t + n - 1, n - 2, 1)
                    ctx.fillRect(l, t + 1, 1, n - 2); ctx.fillRect(l + n - 1, t + 1, 1, n - 2)
                    ctx.fillRect(l + 1, t + 1, n / 2 - 1, n - 2); break
                default:                                     // ○ ring (idle)
                    ctx.fillRect(l + 1, t, n - 2, 1); ctx.fillRect(l + 1, t + n - 1, n - 2, 1)
                    ctx.fillRect(l, t + 1, 1, n - 2); ctx.fillRect(l + n - 1, t + 1, 1, n - 2)
                }
            }
        }
    }
    Text {
        visible: patch.model.hidden > 0
        anchors.right: parent.right
        y: -2
        text: "+" + patch.model.hidden
        color: patch.kit.number
        font: patch.kit.font
        textFormat: Text.PlainText
        style: Text.Outline
        styleColor: patch.kit.withA(color, 0.18)
    }
    MouseArea {
        id: patchMa
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: patch.activated()
    }

    // ── lamps ────────────────────────────────────────────────────────────────
    property var _lamps: ({})
    property int _inFlight: 0
    function activity(ids) {
        for (var i = 0; i < ids.length; i++) {
            var id = ids[i], a = patch.model.at[id]
            if (!a) continue                                // not drawn (overflow, a shell, ended)
            if (patch._lamps[id]) continue                  // coalesce into the one in flight
            if (patch._inFlight >= patch.maxLamps) return   // the cap: later advances drop
            var o = null
            if (a.row === 0)
                o = flashComp.createObject(patch, { key: id, x: a.x - patch.dotS / 2, y: patch.upperY, dur: patch.lampMs })
            else if (patch.model.paths[id])
                o = lampComp.createObject(patch, { key: id, pts: patch.model.paths[id], dur: patch.lampMs })
            if (!o) continue
            patch._lamps[id] = o
            patch._inFlight++
        }
    }
    function lampDone(key, o) {
        delete patch._lamps[key]
        patch._inFlight = Math.max(0, patch._inFlight - 1)
        o.destroy()
    }
    // a root's own activity: its dot lit amber once, for one lamp's time
    Component {
        id: flashComp
        Rectangle {
            id: flash
            property string key: ""
            property int dur: 600
            width: patch.dotS; height: patch.dotS
            color: patch.kit.hot
            NumberAnimation on opacity {
                id: hold
                running: false
                from: 1; to: 1; duration: flash.dur
                onFinished: patch.lampDone(flash.key, flash)
            }
            Component.onCompleted: hold.start()
        }
    }
    // a child reporting upward: a 6px amber dash (3px on a riser) along its wire
    Component {
        id: lampComp
        Rectangle {
            id: lamp
            property string key: ""
            property var pts: []
            property int dur: 600
            property real t: 0
            readonly property var segs: {
                var out = [], total = 0
                for (var i = 1; i < pts.length; i++) {
                    var len = Math.abs(pts[i].x - pts[i - 1].x) + Math.abs(pts[i].y - pts[i - 1].y)
                    out.push({ a: pts[i - 1], b: pts[i], s: total, len: len })
                    total += len
                }
                return { list: out, total: total }
            }
            readonly property var at: {
                var S = segs.list, d = t * segs.total
                for (var i = 0; i < S.length; i++) {
                    var g = S[i]
                    if (d <= g.s + g.len || i === S.length - 1) {
                        var u = g.len > 0 ? Math.min(1, Math.max(0, (d - g.s) / g.len)) : 0
                        return { x: g.a.x + (g.b.x - g.a.x) * u, y: g.a.y + (g.b.y - g.a.y) * u,
                                 horiz: g.a.y === g.b.y && g.len > 0,
                                 x0: Math.min(g.a.x, g.b.x), x1: Math.max(g.a.x, g.b.x),
                                 y0: Math.min(g.a.y, g.b.y), y1: Math.max(g.a.y, g.b.y) }
                    }
                }
                return { x: 0, y: 0, horiz: true, x0: 0, x1: 0, y0: 0, y1: 0 }
            }
            width: at.horiz ? 6 : patch.wireW
            height: at.horiz ? patch.wireW : 3
            x: Math.round(at.horiz ? Math.max(at.x0, Math.min(at.x - 2, at.x1 + patch.wireW - width)) : at.x)
            y: Math.round(at.horiz ? at.y : Math.max(at.y0, Math.min(at.y - 1, at.y1 + patch.wireW - height)))
            color: patch.kit.hot
            NumberAnimation on t {
                id: run
                running: false
                from: 0; to: 1; duration: lamp.dur; easing.type: Easing.Linear
                onFinished: patch.lampDone(lamp.key, lamp)
            }
            Component.onCompleted: run.start()
        }
    }

    // ── the hover pane: the agents with their state (the board's AGENTS, small)
    readonly property int paneCols: 34
    readonly property int paneMax: 14
    property Component paneComponent: Component {
        Use {
            kit: patch.kit; helper: "Pane"
            props: ({
                title: "agents", focused: true, animateOnCreate: true, cols: patch.paneCols,
                stat: Qt.binding(() => patch.model.working + "/" + patch.model.total),
                rows: Qt.binding(() => Math.max(1, Math.min(patch.model.list.length, patch.paneMax)
                                                   + (patch.model.list.length > patch.paneMax ? 1 : 0))),
                content: listBody })
        }
    }
    Component {
        id: listBody
        Column {
            Text {
                visible: patch.model.list.length === 0
                text: "no live agents"
                color: patch.kit.dim; font: patch.kit.font; textFormat: Text.PlainText
            }
            Repeater {
                model: patch.model.list.slice(0, patch.paneMax)
                delegate: Item {
                    id: row
                    required property var modelData
                    readonly property var s: modelData.s
                    readonly property string lead: modelData.depth > 0 ? Kit.rep(" ", 2 * (modelData.depth - 1)) + "└ " : ""
                    width: patch.kit.cells(patch.paneCols)
                    height: patch.kit.cellH
                    Text {
                        text: row.lead
                        color: patch.kit.dim; font: patch.kit.font; textFormat: Text.PlainText
                    }
                    Text {
                        x: patch.kit.cells(row.lead.length)
                        text: patch.kit.lampGlyph(row.s.state)
                        color: patch.kit.lampColor(row.s.state)
                        font: patch.kit.font; textFormat: Text.PlainText
                    }
                    Text {
                        x: patch.kit.cells(row.lead.length + 2)
                        width: patch.kit.cells(patch.paneCols - row.lead.length - 2 - 10) + 2
                        text: ("" + (row.s.name || row.s.agent || "agent")).replace(/\s+/g, " ")
                        color: patch.kit.ink
                        elide: Text.ElideRight
                        font: patch.kit.font; textFormat: Text.PlainText
                    }
                    Text {
                        anchors.right: parent.right
                        text: row.s.state
                        color: row.s.state === "awaiting" ? patch.kit.urgent : patch.kit.mid
                        font: patch.kit.font; textFormat: Text.PlainText
                    }
                }
            }
            Text {
                visible: patch.model.list.length > patch.paneMax
                text: "+" + (patch.model.list.length - patch.paneMax) + " more"
                color: patch.kit.number; font: patch.kit.font; textFormat: Text.PlainText
            }
        }
    }
}
