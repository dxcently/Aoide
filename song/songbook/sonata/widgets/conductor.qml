// conductor.qml — sonata's "conductor" slot: the agent roster temple
// (pantheon · Attic gold · 𝄞), in the dock's gadget column (WidgetSlot host
// — root is an Item that sizes itself off its content, same as every other
// slot in this directory).
//
// Ported from the facet's ConductorGadget.qml (per-song widget-slot
// expansion, CONTRACTS.md §5) — ownership moved from the facet to sonata's
// score. Live: sonata's own `widgets/dock.qml` embeds
// `WidgetSlot { slot: "conductor" }` in its gadget column (slots.md's wired
// table). The facet's ConductorGadget.qml stays in the tree but is no
// longer instantiated by anything.
//
// THE PROGRAM — the roster reads as a CONCERT PLAYBILL: a pantheon entablature
// over a hall of collapsible per-project FOLDERS, each folder a numbered
// movement whose cards are main-agent plaques with their subagent plaques hung
// beneath. Self-contained: this file owns its data read (state/stage/
// sessions.json + projects.json + hooks.json + herald.json, QS_STAGE
// honoured), its model
// build, and its card UI. Nothing here is shared with TerminalsGadget — that temple keeps its
// own, untouched pattern.
//
// ── The entablature (the pantheon) ─────────────────────────────────────────
// PEDIMENT — a triangular tympanum with raking cornices + a marcato-diamond
// acroterion, over a marble inscription band: 𝄞 CONDUCTOR [ conductor ].
// THE SCORE COURSE — the Moonlight motif: five staff hairlines, the C♯ minor
// key signature (F♯5·C♯5·G♯5·D♯5), nine 𝅘𝅥𝅯 sixteenths in three triplet groups,
// a small "3" above each group, and the CONDUCTOR'S BATON laid across the
// score (grip cork → shaft → take-over knot → tip diamond) pointing at the
// final triplet. THE PROGRAM line (┌─┤ ♪ program ├…┐) closes the entablature.
// Measured glyph quirks (Noto Music): the 𝅘𝅥𝅯 head rides ~3.9px above the
// baseline at 16px — the flip's origin.y AND y MUST share the compensation;
// the ♯ glyph rides ~13.25px above the Text top at 11px — y = staffY − 13.25.
//
// ── Projects are MOVEMENTS, collapsible FOLDERS ────────────────────────────
// Each registered project with live sessions gets a numbered folder header —
// `▾ I · AOIDE ──────── 1.2M tok` — in the project's own identity hue. The
// header carries ONE tally by directive: the project's TOTAL TOKEN SUM
// (ctxCompact, "tok" suffix) — no bar, no pct, no count. A chevron ▾/▸ marks
// the fold; clicking the header collapses/expands the folder's rows (snap,
// no height animation — the rows bay height binds to the open state and clips).
// Sessions outside every project fall into an unnumbered dim UNANCHORED
// folder; with NO projects registered no folder chrome draws — just plaques.
// Subagents hang beneath their nearest non-subagent ancestor (a └ hanger in
// the gutter), clamped by climbing the parent chain.
//
// ── The card layout ──────────────────────────────────────────────────────
// See SessionCard's own header comment (near its declaration, below) for
// the current per-row layout — kept there and not duplicated here so this
// section can't drift out of sync with the component again.
//
// ── Reserved space (the anchoring law — text AND animations) ───────────────
// Animated elements stay in reserved boxes. The LAMP (the §2 contract:
// ♪ 𝄐 𝄼 𝄽 𝄂 · state colours — working pulses via a scale animation, awaiting
// breathes via an opacity animation) lives INSIDE a fixed 16px box at the
// card's top-left; both animations are confined to the box — nothing floats,
// no layout shift. The π THINK-TAG (hooked main pi sessions only, visible
// only while the live state is working) lives in a fixed 13×16 box on the
// identity row, pen confined to the box. A main Codex agent carries a small
// illuminated book in a 24×18 box: pages turn while working, the open spread
// rests on a hold, and the cover closes at idle/stopped/done. Its name keeps
// the same reserved width in every state. The KAOMOJI
// TROUPE lives inside a fixed clipped box in the
// ground row (116px main / 90px sub); its frames swap text, never geometry.
// The thinking slot itself is a fixed-height reserved lane. Every free-
// length string elides against a fixed partner: name vs tags, model/id
// middle-out, thinking against its own lane, cwd left-elided. Every text
// anchors to EXACTLY one side (never centred) — fixed columns, fixed order.
// Hover previews the card's workspace + takes the trace; click →
// bridge.focusSession (a courier focuses its parent's window). One laurel
// standout (§5): traced card, else first working. All colour from `livery`;
// radius 0 everywhere.
//
// ── The hooks channel (any-agent) ──────────────────────────────────────────
// state/stage/hooks.json — [{ sessionId, phase, updatedAt }] — is agent-agnostic:
// ANY agent session may be hooked into a phase from outside the roster's own
// state. The hook phase WINS as the card's live state (cardLiveState): it
// drives the lamp glyph/colour, the metronome pulse, the terracotta breath,
// the border, and the kaomoji face — while laurel/firstWorkingId/workingCount
// stay roster-based (stable tallies). Hook surfacing: a ϟ tag on the identity
// row — replaced by the π think-tag for hooked MAIN pi sessions (piTag
// below: exists ONLY while working — the self-writing loop; at rest it
// vanishes; the ϟN tally and the "ϟ phase" placeholder keep ϟ) — plus a
// "ϟ phase"
// placeholder in the thinking box when the agent has not
// spoken, and a ϟN count in the stylobate tally. Phases outside the §2
// vocabulary fall back to the "·" lamp and the puzzled still — tolerant,
// never a crash. Stale hook ids (absent from the roster) are ignored: they
// produce no rows and never inflate the ϟN tally.
//
// ── Reaping ────────────────────────────────────────────────────────────────
// The liveness reaper (`aoide session reap`, ~12s timer) sweeps dead sessions
// out of the stage file out-of-band — the widget needs NO tombstone rows: a
// session gone from sessions.json is gone from the roster on the next FileView
// reload; `state: "done"` records stay visible with the done pose (𝄂 lamp,
// ( ´▽｀ ) face) until they leave the file. Stale hooks never resurrect rows;
// rebuild() is crash-free on partial/empty records — every stage field is
// optional (additive-v0 contract).

import QtQuick
import Quickshell
import Quickshell.Io
// Reaches the facet's shared components — MoodFaces (the kaomoji troupe)
// and ScrollRail (the draggable scrollbar) — neither of which is
// conductor-specific chrome, so they stay in the facet rather than moving
// here. Deployed-tree relative path: this file lands at $out/qml/songs/
// sonata/conductor.qml, so two levels up ($out/qml/songs/ → $out/qml/) is
// the facet's own qml/ root (bar.qml idiom; see that file's own header).
import "../.."

Item {
    id: temple

    // ── Integration contract ────────────────────────────────────────────────
    required property var livery
    required property var bridge
    property var shared: null

    Loader {
        id: sessionMenu
        anchors.fill: parent
        z: 100
        source: Qt.resolvedUrl("SessionMenu.qml")
        onLoaded: { item.livery = temple.livery; item.bridge = temple.bridge }
    }
    function openSessionMenu(record, sourceItem, x, y) {
        if (!sessionMenu.item) return
        var point = sourceItem.mapToItem(temple, x, y)
        var host = null
        var parentId = record && record.parentSessionId
        if (parentId) {
            var all = temple._sessions || []
            for (var i = 0; i < all.length; i++) {
                if (all[i] && all[i].sessionId === parentId) { host = all[i]; break }
            }
        }
        sessionMenu.item.open(record, point.x, point.y, host)
    }

    implicitWidth: 360
    implicitHeight: 520

    // Doric order — Attic gold signature
    readonly property color signature: livery.paletteAccent

    // type voices — the shared three
    readonly property string faceSerif: "Noto Serif"
    readonly property string faceMono:  "JetBrainsMono Nerd Font"
    readonly property string faceMusic: "Noto Music"
    readonly property string faceSymbol: "Noto Sans Symbols 2"   // hookTag's working-spinner
    readonly property string faceEmoji:  "Noto Color Emoji"      // moonTag's lunation

    function withA(cstr, a) {
        var c = Qt.darker(cstr, 1.0)
        return Qt.rgba(c.r, c.g, c.b, a)
    }

    // Transform LISTS are fixed on this host and are not bindable in Qt.
    // Discover their objects at attachment; bind to each Translate's x/y below.
    function markTransforms(mark) {
        var out = []
        for (var node = mark; node; node = node.parent)
            for (var i = 0; i < node.transform.length; i++)
                out.push(node.transform[i])
        return out
    }

    // Item.visible stays true behind a clip or the dock's translated cover.
    // Snapshot the ancestor geometry explicitly: mapToItem() alone does not
    // register those dependencies for a QML binding. The current host's
    // transform list contains Translate; its x/y need their own reads too.
    function markExposed(mark, win, shifts) {
        if (!mark || !mark.visible || !win || !win.visible) return false
        var positions = []
        for (var i = 0; i < shifts.length; i++)
            positions.push([shifts[i].x, shifts[i].y])
        var chain = []
        for (var node = mark; node; node = node.parent) {
            chain.push({ item: node, parent: node.parent,
                x: node.x, y: node.y, width: node.width, height: node.height,
                visible: node.visible, opacity: node.opacity, clip: node.clip,
                scale: node.scale, rotation: node.rotation,
                transformOrigin: node.transformOrigin })
        }
        var rect = Qt.rect(0, 0, mark.width, mark.height)
        for (var j = 0; j < chain.length; j++) {
            var g = chain[j]
            if (!g.visible || g.opacity <= 0) return false
            if (g.clip) {
                var left = Math.max(0, rect.x), top = Math.max(0, rect.y)
                var right = Math.min(g.width, rect.x + rect.width)
                var bottom = Math.min(g.height, rect.y + rect.height)
                if (right <= left || bottom <= top) return false
                rect = Qt.rect(left, top, right - left, bottom - top)
            }
            rect = g.item.mapToItem(g.parent, rect)
        }
        return rect.width > 0 && rect.height > 0
            && rect.x < win.width && rect.y < win.height
            && rect.x + rect.width > 0 && rect.y + rect.height > 0
    }

    // ── The kaomoji troupe + its beat ───────────────────────────────────────
    MoodFaces { id: faces }
    property int faceTick: 0
    Timer {
        // Runs whenever the stele is visible — NOT gated on roster working
        // count: a hooked-working session whose roster state is idle must
        // still animate its face (the hook wins the live state, B4).
        interval: 420
        running: temple.visible
        repeat: true
        onTriggered: temple.faceTick++
    }

    // elapsed tallies tick without a file change
    property real nowMs: Date.now()
    Timer { interval: 10000; running: temple.visible; repeat: true
            onTriggered: temple.nowMs = Date.now() }

    // ── Stage files (QS_STAGE override — the preview-harness seam) ──────────
    // CONDUCTING files (sessions/projects/hooks/herald) live under
    // state/stage/, not song/stage/ (CONTRACTS.md §4, command-defrag S2) —
    // this temple never reads a rice file.
    readonly property string stageDir: {
        var s = Quickshell.env("QS_STAGE")
        return (s && s.length > 0) ? s
             : (Quickshell.env("AOIDE_ROOT") || (Quickshell.env("HOME") + "/.aoide")) + "/state/stage"
    }

    property var _sessions: []
    property var _projects: []
    property var _hooks: []
    property var hookById: ({})

    FileView {
        id: sessionsFile
        path: temple.stageDir + "/sessions.json"
        watchChanges: true
        blockLoading: false
        printErrors: false
        onLoaded: temple.parseSessions()
        onFileChanged: reload()
    }
    FileView {
        id: projectsFile
        path: temple.stageDir + "/projects.json"
        watchChanges: true
        blockLoading: false
        printErrors: false
        onLoaded: temple.parseProjects()
        onFileChanged: reload()
    }
    FileView {
        id: hooksFile
        path: temple.stageDir + "/hooks.json"
        watchChanges: true
        blockLoading: false
        printErrors: false
        onLoaded: temple.parseHooks()
        onFileChanged: reload()
    }
    // ── The SUMMONS channel — the herald ledger, read for permission cards ──
    // state/stage/herald.json is the herald's own file (`aoide herald` / `session
    // permit` write it through the shellbridge). The roster reads ONLY its
    // `kind: "summons"` records, and only to learn which session is blocked on
    // a permission prompt the human can actually answer.
    //
    // Why the ledger and not `state === "awaiting"`: a summons exists only for
    // a session `session permit` found CONDUCTABLE — one with a control socket to
    // type the answer into. An awaiting session with no summons has no channel,
    // and drawing it a verdict button would be a live-looking control that
    // cannot do anything (permit.rs, guard 1). The record also disappears the
    // moment the verdict lands (the daemon dismisses the card after answering),
    // so the chips take themselves down with no local latch.
    FileView {
        id: heraldFile
        path: temple.stageDir + "/herald.json"
        watchChanges: true
        blockLoading: false
        printErrors: false
        onLoaded: temple.parseHerald()
        onFileChanged: reload()
    }
    Component.onCompleted: { parseSessions(); parseProjects(); parseHooks(); parseHerald() }

    function parseSessions() {
        try {
            var t = sessionsFile.text()
            var o = (t && t.trim().length > 0) ? JSON.parse(t) : null
            temple._sessions = (o && o.sessions) ? o.sessions : []
        } catch (e) { temple._sessions = [] }
        rebuild()
    }
    function parseProjects() {
        try {
            var t = projectsFile.text()
            var o = (t && t.trim().length > 0) ? JSON.parse(t) : null
            temple._projects = (o && o.projects) ? o.projects : []
        } catch (e) { temple._projects = [] }
        rebuild()
    }
    function parseHooks() {
        try {
            var t = hooksFile.text()
            var o = (t && t.trim().length > 0) ? JSON.parse(t) : null
            var arr = (o && o.hooks) ? o.hooks : []
            temple._hooks = arr
            var byId = {}
            for (var i = 0; i < arr.length; i++) {
                var h = arr[i]
                if (!h || !h.sessionId) continue
                byId[h.sessionId] = {
                    phase: ("" + (h.phase || "")).toLowerCase(),
                    updatedAt: h.updatedAt || ""
                }
            }
            temple.hookById = byId
        } catch (e) {
            temple._hooks = []
            temple.hookById = {}
        }
        rebuild()
    }

    // Summons records keyed by the SESSION they block. A verdict is addressed
    // to the session, so that is the key; the ledger's own record id
    // (`permit-<session>`) is the daemon's business and never leaves it.
    property var summonsById: ({})
    function parseHerald() {
        var byId = {}
        try {
            var t = heraldFile.text()
            var o = (t && t.trim().length > 0) ? JSON.parse(t) : null
            var arr = (o && o.notifications) ? o.notifications : []
            for (var i = 0; i < arr.length; i++) {
                var n = arr[i]
                if (!n || n.kind !== "summons" || !n.sessionId) continue
                byId[n.sessionId] = n     // newest wins; the ledger appends
            }
        } catch (e) { byId = {} }
        temple.summonsById = byId
    }

    // The ONE outbound door for a verdict — the same wire line the herald's own
    // chips send ({ cmd: "heraldverdict", id: <sessionId>, verdict: … }), so
    // both surfaces answer through one daemon path, with one audit trail and
    // one still-awaiting guard. The closed verdict vocabulary is enforced
    // daemon-side; nothing else is ever sent from here.
    function verdict(sessionId, word) {
        if (!temple.bridge || !sessionId) return
        temple.bridge.sendCommand({ cmd: "heraldverdict",
                                    id: "" + sessionId, verdict: word })
    }

    // The hook lookup — the any-agent seam. hookPhase returns "" (falsy) for a
    // session with no hook, so cardLiveState falls through to the roster state.
    function hookPhase(id) {
        var h = temple.hookById[id || ""]
        return h ? h.phase : ""
    }
    function hooked(id) {
        return !!temple.hookById[id || ""]
    }

    // ── Pure formatting helpers (no invented business logic) ────────────────

    function kindOf(s) {
        if (s && s.kind) return s.kind
        return (s && s.agent === "shell") ? "shell" : "agent"
    }

    // The card's display name: its own title when the daemon published one,
    // else the agent string. NEVER the model id — that renders in full, as
    // its own identity line, unabbreviated.
    function rowName(s) {
        if (!s) return ""
        if (s.title) return s.title
        if (kindOf(s) === "subagent") return s.agent ? s.agent : "subagent"
        return s.agent || "agent"
    }

    // cwd breadcrumb: home-relative ("~"), then the FIRST segment, then "…/",
    // then the LAST TWO segments — `~/Aoide/…/quickshell/qml`. Paths of ≤3
    // segments show whole; directly in $HOME → "~". The renderer ADDITIONALLY
    // left-elides, so even a long tail keeps its last segment visible.
    function shortPath(p) {
        if (!p) return ""
        var s = "" + p
        var home = Quickshell.env("HOME")
        if (home && s.indexOf(home) === 0) s = "~" + s.substring(home.length)
        var isTilde = s.charAt(0) === "~"
        var abs = s.charAt(0) === "/"
        var parts = s.split("/").filter(function (x) { return x.length > 0 })
        if (isTilde) parts.shift()          // "~" becomes the prefix
        if (parts.length > 3)
            return (isTilde ? "~/" : (abs ? "/" : ""))
                 + parts[0] + "/…/" + parts.slice(parts.length - 2).join("/")
        return s
    }

    // ── §2 state contract — glyph + colour, verbatim ────────────────────────
    function lampGlyph(st) {
        switch (st) {
        case "working":  return "♪"
        case "awaiting": return "𝄐"
        case "stopped":  return "𝄼"
        case "idle":     return "𝄽"
        case "done":     return "𝄂"
        default:         return "·"
        }
    }
    function lampColor(st) {
        switch (st) {
        case "working":  return livery.paletteAccent
        case "awaiting": return livery.paletteUrgent
        case "stopped":  return livery.holoBlue
        case "idle":     return livery.violet
        case "done":     return livery.wireCyan
        default:         return livery.paletteFg
        }
    }

    // movement numerals — plain ASCII (no glyph-coverage gamble)
    function roman(n) {
        var r = ["", "I", "II", "III", "IV", "V", "VI",
                 "VII", "VIII", "IX", "X", "XI", "XII"]
        return (n >= 1 && n <= 12) ? r[n] : ("" + n)
    }

    // ── The movement/folder model ───────────────────────────────────────────
    // groups: [{ name, ord (movement numeral, 0 = unnumbered), hueIndex,
    //            anchored, rows: [{ s, kids: [session…] }], items: [all] }]
    property var groups: []
    property string firstWorkingId: ""
    property int totalCount: 0
    property int workingCount: 0
    property int totalTokens: 0
    property int hookedCount: 0

    // Collapse state — keyed by project name ("adrift" for the unanchored
    // folder); survives rebuilds because it lives on the root, not the model.
    property var collapsed: ({})
    function isOpen(g) { return !temple.collapsed[g.name || "adrift"] }

    function byStart(a, b) {
        var x = (a && a.startedAt) || "", y = (b && b.startedAt) || ""
        return x < y ? -1 : (x > y ? 1 : 0)
    }

    function projectOf(cwd) {
        if (!cwd) return -1
        var best = -1, bestLen = -1
        var ps = _projects || []
        for (var i = 0; i < ps.length; i++) {
            var roots = ps[i].roots && ps[i].roots.length ? ps[i].roots : [ps[i].path]
            for (var j = 0; j < roots.length; j++) {
                var p = roots[j]
                if (!p) continue
                if ((cwd === p || cwd.indexOf(p === "/" ? "/" : p + "/") === 0) && p.length > bestLen) {
                    best = i; bestLen = p.length
                }
            }
        }
        return best
    }

    function rebuild() {
        var all = _sessions || []
        var mine = []
        for (var i = 0; i < all.length; i++) {
            var s = all[i]
            if (!s) continue
            // The daemon owns liveness; this roster shows its live states only.
            if (s.state !== "working" && s.state !== "awaiting"
                    && s.state !== "stopped" && s.state !== "idle") continue
            if (kindOf(s) !== "shell") mine.push(s)   // agents · subagents
        }

        var byId = {}
        for (i = 0; i < mine.length; i++) byId[mine[i].sessionId] = mine[i]

        // Split tops from subagent children; clamp depth by climbing to the
        // nearest NON-subagent ancestor present in the roster.
        var tops = [], kids = {}
        for (i = 0; i < mine.length; i++) {
            var s2 = mine[i]
            if (kindOf(s2) === "subagent") {
                var anchor = null, cur = s2, hops = 0
                while (cur && cur.parentSessionId && hops < 8) {
                    var par = byId[cur.parentSessionId]
                    if (!par) break
                    if (kindOf(par) !== "subagent") { anchor = par; break }
                    cur = par; hops++
                }
                if (anchor) {
                    if (!kids[anchor.sessionId]) kids[anchor.sessionId] = []
                    kids[anchor.sessionId].push(s2)
                    continue
                }
            }
            tops.push(s2)   // top-level session, or an orphaned courier
        }
        tops.sort(byStart)
        for (var kk in kids) kids[kk].sort(byStart)

        // Bucket tops (children ride with their parent) into movements. Stamp a
        // UI-only graph ordinal (`.no`) as we go: TOPS get a running #N in
        // byStart order — subagents never consume a slot of their own. Each
        // top's kids instead get that SAME number suffixed by their own
        // 1-based index under their parent (`N.1`, `N.2`, …), so a subagent
        // always reads as "hanging off top N", never as its own top-level
        // agent. Cosmetic and stable within a session, NOT a persisted id;
        // SessionCard reads it as s.no.
        var buckets = [], ps = _projects || []
        for (i = 0; i < ps.length; i++) buckets.push({ rows: [], items: [] })
        var adrift = { rows: [], items: [] }
        var seq = 0
        for (i = 0; i < tops.length; i++) {
            var t = tops[i]
            var pi = projectOf(t.cwd)
            if (t.project) {
                pi = -1
                for (var p = 0; p < ps.length; p++) if (ps[p].name === t.project) { pi = p; break }
            }
            var ch = kids[t.sessionId] || []
            var target = (pi >= 0) ? buckets[pi] : adrift
            t.no = ++seq
            target.rows.push({ s: t, kids: ch })
            target.items.push(t)
            for (var j = 0; j < ch.length; j++) {
                ch[j].no = t.no + "." + (j + 1)
                target.items.push(ch[j])
            }
        }

        var gs = [], ord = 0
        for (i = 0; i < ps.length; i++) {
            if (buckets[i].rows.length === 0) continue
            ord++
            gs.push({ name: (ps[i].name || "project"), ord: ord, hueIndex: i + 1,
                      anchored: true, rows: buckets[i].rows, items: buckets[i].items })
        }
        if (adrift.rows.length > 0) {
            // The only-group case (no projects registered) draws no movement
            // chrome at all — the playbill is just plaques.
            gs.push({ name: gs.length > 0 ? "unanchored" : "", ord: 0, hueIndex: 0,
                      anchored: false, rows: adrift.rows, items: adrift.items })
        }

        // Tallies + the default laurel (first working top-level plaque).
        // totalCount/workingCount/firstWorkingId stay ROSTER-based (stable
        // tallies); totalTokens sums over EVERY roster item including kids
        // (both folders and adrift); hookedCount counts only roster items with
        // a hook — stale hook ids are ignored, they never inflate the ϟN.
        var total = 0, work = 0, fw = "", toks = 0, hookedN = 0
        for (i = 0; i < gs.length; i++) {
            var rows = gs[i].rows
            for (j = 0; j < rows.length; j++) {
                var r = rows[j]
                total++
                toks += (r.s && r.s.contextTokens) || 0
                if (temple.hooked(r.s ? r.s.sessionId : "")) hookedN++
                if (r.s.state === "working") {
                    work++
                    if (fw === "") fw = r.s.sessionId || ""
                }
                for (var q = 0; q < r.kids.length; q++) {
                    total++
                    toks += (r.kids[q].contextTokens) || 0
                    if (temple.hooked(r.kids[q].sessionId)) hookedN++
                    if (r.kids[q].state === "working") work++
                }
            }
        }
        temple.groups = gs
        temple.totalCount = total
        temple.workingCount = work
        temple.firstWorkingId = fw
        temple.totalTokens = toks
        temple.hookedCount = hookedN
    }

    // The one laurel standout (§5): the traced card when it lives in THIS
    // roster, else the first working top-level card.
    readonly property string emphasizedId: {
        var gs = temple.groups
        var t = (temple.shared && temple.shared.tracedSessionId)
                ? temple.shared.tracedSessionId : ""
        if (t !== "") {
            for (var i = 0; i < gs.length; i++) {
                var rows = gs[i].rows
                for (var j = 0; j < rows.length; j++) {
                    if (rows[j].s.sessionId === t) return t
                    for (var q = 0; q < rows[j].kids.length; q++)
                        if (rows[j].kids[q].sessionId === t) return t
                }
            }
        }
        return temple.firstWorkingId
    }

    // ═════ THE STELE ═════════════════════════════════════════════════════════

    // cast shadow — lifts the stele off the marble
    Rectangle {
        anchors.fill: stele
        anchors.leftMargin: 4; anchors.topMargin: 5
        anchors.rightMargin: -4; anchors.bottomMargin: -5
        radius: 0
        color: temple.withA(livery.paletteFg, 0.22)
    }

    Rectangle {
        id: stele
        anchors.fill: parent
        radius: 0
        color: livery.paletteBg
        border.color: livery.paletteFg
        border.width: 2

        Rectangle {                              // inset keyline — Attic gold
            anchors.fill: parent; anchors.margins: 4
            radius: 0; color: "transparent"
            border.color: temple.signature; border.width: 1
        }

        // ── ENTABLATURE — the pantheon ───────────────────────────────────────
        Column {
            id: head
            anchors.left: parent.left; anchors.right: parent.right
            anchors.top: parent.top
            anchors.margins: 13
            spacing: 5

            // ── A2 · pediment + inscription band (h53) ───────────────────────
            Item {
                width: parent.width
                height: 53

                Rectangle {                      // deeper marble band
                    anchors.top: pediment.bottom
                    anchors.left: parent.left; anchors.right: parent.right
                    anchors.bottom: parent.bottom; anchors.bottomMargin: 4
                    color: temple.withA(livery.paletteFg, 0.05)
                }

                // the pediment — tympanum, raking cornices, acroterion diamond
                Canvas {
                    id: pediment
                    anchors.top: parent.top
                    width: parent.width
                    height: 18
                    onWidthChanged: requestPaint()
                    onPaint: {
                        var ctx = getContext("2d")
                        ctx.reset()
                        var w = width, h = height
                        var baseY = h - 1.4, apexX = w / 2, apexY = 5.5
                        // tympanum field
                        ctx.beginPath()
                        ctx.moveTo(2, baseY)
                        ctx.lineTo(apexX, apexY)
                        ctx.lineTo(w - 2, baseY)
                        ctx.closePath()
                        ctx.fillStyle = temple.withA(temple.signature, 0.07)
                        ctx.fill()
                        // raking cornices + the taenia (base line)
                        ctx.beginPath()
                        ctx.moveTo(2, baseY); ctx.lineTo(apexX, apexY)
                        ctx.moveTo(w - 2, baseY); ctx.lineTo(apexX, apexY)
                        ctx.moveTo(2, baseY); ctx.lineTo(w - 2, baseY)
                        ctx.strokeStyle = temple.withA(temple.signature, 0.8)
                        ctx.lineWidth = 1.1
                        ctx.stroke()
                        // the marcato-diamond acroterion at the apex
                        ctx.beginPath()
                        ctx.moveTo(apexX, apexY - 5)
                        ctx.lineTo(apexX + 2.2, apexY - 2.8)
                        ctx.lineTo(apexX, apexY - 0.6)
                        ctx.lineTo(apexX - 2.2, apexY - 2.8)
                        ctx.closePath()
                        ctx.fillStyle = temple.signature
                        ctx.fill()
                    }
                }

                // the inscription band — 𝄞 CONDUCTOR [ conductor ]
                Item {
                    anchors.left: parent.left; anchors.right: parent.right
                    anchors.bottom: parent.bottom
                    height: 34

                    Text {
                        id: crownGlyph
                        anchors.left: parent.left
                        anchors.verticalCenter: parent.verticalCenter
                        anchors.verticalCenterOffset: -2
                        text: "𝄞"
                        font.family: temple.faceMusic; font.pixelSize: 24
                        color: temple.signature
                    }
                    Text {
                        anchors.left: crownGlyph.right; anchors.leftMargin: 10
                        anchors.verticalCenter: parent.verticalCenter
                        text: "CONDUCTOR"
                        font.family: temple.faceSerif; font.pixelSize: 17
                        font.weight: Font.DemiBold; font.letterSpacing: 4
                        color: livery.paletteFg
                    }
                    Text {                       // the source tag IS the recheck
                                                 // button — UsageGadget's hover-
                                                 // morph idiom, one reserved
                                                 // right-anchored slot: hovering
                                                 // swaps the inscription for the
                                                 // click cue, a click fires an
                                                 // on-demand reap/rehook sweep
                                                 // instead of the daemon's ~12s
                                                 // timer. Guarded like
                                                 // focusSession — a no-op until
                                                 // the bridge command lands.
                        id: condTag
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        text: condMouse.containsMouse ? "[ reap ]"
                                                      : "[ conductor ]"
                        font.family: temple.faceMono; font.pixelSize: 11
                        color: temple.withA(temple.signature, 0.95)
                        MouseArea {
                            id: condMouse
                            anchors.fill: parent; anchors.margins: -4
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: {
                                if (temple.bridge && temple.bridge.recheckSessions)
                                    temple.bridge.recheckSessions()
                            }
                        }
                    }
                }
            }

            // ── A3 · the score course (h33) — Moonlight + the baton ──────────
            Item {
                id: course
                width: parent.width
                height: 33

                Canvas {                         // staff + the baton
                    anchors.fill: parent
                    onWidthChanged: requestPaint()
                    onPaint: {
                        var ctx = getContext("2d")
                        ctx.reset()
                        // the five staff hairlines
                        ctx.strokeStyle = temple.withA(temple.signature, 0.4)
                        ctx.lineWidth = 1
                        ctx.beginPath()
                        var ys = [7.5, 13, 18.5, 24, 29.5]
                        for (var i = 0; i < ys.length; i++) {
                            ctx.moveTo(0, ys[i]); ctx.lineTo(width, ys[i])
                        }
                        ctx.stroke()
                        // the conductor's baton, laid across the score: grip
                        // cork at the lower right, shaft climbing to the tip
                        // diamond over the final triplet, with a take-over
                        // knot 0.16 of the way up the shaft.
                        var gx = width - 10, gy = 30.5
                        var tx = width - 105, ty = 13.5
                        var kx = gx + (tx - gx) * 0.16
                        var ky = gy + (ty - gy) * 0.16
                        ctx.strokeStyle = temple.withA(temple.signature, 0.95)
                        ctx.lineCap = "round"
                        ctx.beginPath()          // the thin shaft, knot → tip
                        ctx.moveTo(kx, ky); ctx.lineTo(tx, ty)
                        ctx.lineWidth = 1.2
                        ctx.stroke()
                        ctx.beginPath()          // the thick grip cork, grip → knot
                        ctx.moveTo(gx, gy); ctx.lineTo(kx, ky)
                        ctx.lineWidth = 2.4
                        ctx.stroke()
                        ctx.beginPath()          // the tip diamond
                        ctx.moveTo(tx, ty - 1.6)
                        ctx.lineTo(tx + 1.6, ty)
                        ctx.lineTo(tx, ty + 1.6)
                        ctx.lineTo(tx - 1.6, ty)
                        ctx.closePath()
                        ctx.fillStyle = temple.signature
                        ctx.fill()
                    }
                }

                // the key signature — C♯ minor: F♯5 · C♯5 · G♯5 · D♯5. The ♯
                // glyph rides ~13.25px above the Text top in Noto Music at 11px
                // (measured), so y: staffY − 13.25.
                Text { x: 16; y: 7.5 - 13.25
                       text: "\u266F"
                       font.family: temple.faceMusic; font.pixelSize: 11
                       color: temple.signature }
                Text { x: 24; y: 15.75 - 13.25
                       text: "\u266F"
                       font.family: temple.faceMusic; font.pixelSize: 11
                       color: temple.signature }
                Text { x: 32; y: 4.75 - 13.25
                       text: "\u266F"
                       font.family: temple.faceMusic; font.pixelSize: 11
                       color: temple.signature }
                Text { x: 40; y: 13 - 13.25
                       text: "\u266F"
                       font.family: temple.faceMusic; font.pixelSize: 11
                       color: temple.signature }

                // the arpeggio — the opening broken chord: C♯₅ E₅ G♯₅,
                // three triplet groups. sy = staff position of each head:
                // C♯5 on space 3, E5 on space 4, G♯5 floating above the top.
                property var moonlight: [
                    { x: 58,   sy: 15.75 },        // C♯5
                    { x: 70,   sy: 10.25 },        // E5
                    { x: 82,   sy: 4.75 },         // G♯5
                    { x: 102,  sy: 15.75 },        // C♯5
                    { x: 114,  sy: 10.25 },        // E5
                    { x: 126,  sy: 4.75 },         // G♯5
                    { x: 146,  sy: 15.75 },        // C♯5
                    { x: 158,  sy: 10.25 },        // E5
                    { x: 170,  sy: 4.75 }          // G♯5
                ]
                Repeater {
                    model: course.moonlight
                    delegate: Text {
                        required property var modelData
                        x: modelData.x
                        y: modelData.sy - baselineOffset + 3.9
                        text: "\uD834\uDD61"      // 𝅘𝅥𝅯 sixteenth
                        font.family: temple.faceMusic; font.pixelSize: 16
                        color: temple.signature
                        // Stems DOWN — correct engraving for the upper staff.
                        // Noto Music's head rides ~3.9px above the baseline at
                        // 16px (measured); flip origin and y share that
                        // compensation so heads land on their staff position.
                        transform: Scale { yScale: -1; origin.y: baselineOffset - 3.9 }
                    }
                }

                // the triplet marks — a small "3" above each group. These DO
                // render: at 9px the digit is only ~4px of ink, so a scan
                // crop a few pixels off from the window's true position
                // misses it entirely — verify against the staff, not the
                // spec's x.
                Text { x: 67; y: -5.4
                       text: "3"
                       font.family: temple.faceSerif; font.italic: true
                       font.pixelSize: 9
                       color: temple.withA(temple.signature, 0.9) }
                Text { x: 111; y: -5.4
                       text: "3"
                       font.family: temple.faceSerif; font.italic: true
                       font.pixelSize: 9
                       color: temple.withA(temple.signature, 0.9) }
                Text { x: 155; y: -5.4
                       text: "3"
                       font.family: temple.faceSerif; font.italic: true
                       font.pixelSize: 9
                       color: temple.withA(temple.signature, 0.9) }
            }

            // ── A4 · the program line (h14) ──────────────────────────────────
            Item {
                width: parent.width; height: 14
                Text {
                    id: tfL
                    anchors.left: parent.left
                    anchors.verticalCenter: parent.verticalCenter
                    text: "┌─┤ ♪ program ├"
                    font.family: temple.faceMono; font.pixelSize: 11
                    color: temple.withA(temple.signature, 0.95)
                }
                Text {
                    id: tfR
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    text: "┐"
                    font.family: temple.faceMono; font.pixelSize: 11
                    color: temple.withA(temple.signature, 0.95)
                }
                Rectangle {
                    anchors.left: tfL.right; anchors.right: tfR.left
                    anchors.leftMargin: 2; anchors.rightMargin: 2
                    anchors.verticalCenter: parent.verticalCenter
                    anchors.verticalCenterOffset: 1
                    height: 1
                    color: temple.withA(temple.signature, 0.55)
                }
            }
        }

        // ── BODY: the playbill, scrolling ────────────────────────────────────
        Flickable {
            id: flick
            anchors.top: head.bottom; anchors.topMargin: 5
            anchors.bottom: foot.top; anchors.bottomMargin: 3
            anchors.left: parent.left; anchors.leftMargin: 13
            anchors.right: parent.right; anchors.rightMargin: 13
            clip: true
            contentWidth: width
            contentHeight: playbill.height
            boundsBehavior: Flickable.StopAtBounds
            flickDeceleration: 3500

            Column {
                id: playbill
                width: flick.width - 6           // slim gutter for the scrollbar
                spacing: 9

                // ── EMPTY STAGE — TACET ──────────────────────────────────────
                Item {
                    visible: temple.groups.length === 0
                    width: playbill.width
                    height: visible ? 150 : 0
                    Column {
                        anchors.centerIn: parent
                        spacing: 8
                        Text {
                            anchors.horizontalCenter: parent.horizontalCenter
                            text: "T A C E T"
                            font.family: temple.faceSerif; font.pixelSize: 14
                            font.letterSpacing: 6
                            color: temple.withA(livery.paletteFg, 0.45)
                        }
                        Text {
                            anchors.horizontalCenter: parent.horizontalCenter
                            text: "ᕕ( ᐛ )ᕗ"
                            font.family: temple.faceMono; font.pixelSize: 15
                            color: temple.withA(livery.paletteFg, 0.5)
                        }
                        Text {
                            anchors.horizontalCenter: parent.horizontalCenter
                            text: "no agents on stage"
                            font.family: temple.faceSerif; font.italic: true
                            font.pixelSize: 10
                            color: temple.withA(livery.paletteFg, 0.4)
                        }
                    }
                }

                // ── MOVEMENTS (the collapsible project folders) ──────────────
                Repeater {
                    model: temple.groups
                    delegate: Column {
                        id: movement
                        required property var modelData
                        readonly property var g: modelData
                        readonly property color hue:
                            g.anchored ? temple.livery.noteColor(g.hueIndex)
                                       : temple.withA(temple.livery.paletteFg, 0.55)
                        readonly property bool headed: g.name !== ""
                        readonly property var roll: temple.livery.ctxRollup(g.items)
                        readonly property bool open: temple.isOpen(g)

                        width: playbill.width
                        spacing: 4

                        // the folder header — ▾ I · AOIDE ────── 1.2M tok
                        Item {
                            visible: movement.headed
                            width: parent.width
                            height: visible ? 16 : 0

                            Text {               // the fold chevron
                                x: 0
                                anchors.verticalCenter: parent.verticalCenter
                                width: 12
                                horizontalAlignment: Text.AlignLeft
                                text: movement.open ? "▾" : "▸"
                                font.family: temple.faceMono; font.pixelSize: 9
                                color: movement.hue
                            }
                            Text {
                                id: gName
                                anchors.left: parent.left; anchors.leftMargin: 16
                                anchors.verticalCenter: parent.verticalCenter
                                text: (movement.g.anchored
                                       ? temple.roman(movement.g.ord) + " · " : "")
                                      + movement.g.name.toUpperCase()
                                font.family: temple.faceSerif; font.pixelSize: 11
                                font.weight: Font.Medium; font.letterSpacing: 3
                                elide: Text.ElideRight
                                // long project names shrink against the token
                                // sum (the budget covers chevron + rule margins)
                                width: Math.min(implicitWidth,
                                                parent.width - gSum.implicitWidth - 24)
                                color: movement.hue
                            }
                            Text {
                                // THE DIRECTIVE: the header carries the
                                // project's TOTAL TOKEN SUM ONLY — no bar, no
                                // pct, no count.
                                id: gSum
                                anchors.right: parent.right
                                anchors.verticalCenter: parent.verticalCenter
                                text: movement.roll.any
                                       ? (temple.livery.ctxCompact(movement.roll.sumTok)
                                          + " tok")
                                       : "—"
                                font.family: temple.faceMono; font.pixelSize: 9
                                color: movement.roll.any
                                       ? temple.withA(temple.livery.paletteAccent, 0.9)
                                       : temple.withA(temple.livery.paletteFg, 0.5)
                            }
                            Rectangle {
                                anchors.left: gName.right; anchors.leftMargin: 8
                                anchors.right: gSum.left; anchors.rightMargin: 8
                                anchors.verticalCenter: parent.verticalCenter
                                height: 1
                                color: temple.withA(movement.hue, 0.3)
                            }

                            // hover ground + the fold toggle
                            Rectangle {
                                anchors.fill: parent
                                visible: foldMouse.containsMouse
                                color: temple.withA(movement.hue, 0.08)
                            }
                            MouseArea {
                                id: foldMouse
                                anchors.fill: parent
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                onClicked: {
                                    // Reassign (not mutate) collapsed so the
                                    // property-change notification actually
                                    // fires — QML tracks binding deps at the
                                    // property level, not deep object
                                    // mutation, so bracket-assigning a key in
                                    // place left movement.open (and every
                                    // binding fed by isOpen()) stale until an
                                    // unrelated rebuild() reassigned groups
                                    // and forced fresh evaluation.
                                    var key = movement.g.name || "adrift"
                                    var next = {}
                                    for (var k in temple.collapsed)
                                        next[k] = temple.collapsed[k]
                                    next[key] = !next[key]
                                    temple.collapsed = next
                                }
                            }
                        }

                        // the folder's rows — deterministic collapse: the bay
                        // height binds to the open state and clips, so the
                        // layout is a snap (no height animation; park one as a
                        // future nicety).
                        Item {
                            id: rowsBay
                            width: parent.width
                            height: movement.open ? rowsCol.implicitHeight : 0
                            clip: true

                            Column {
                                id: rowsCol
                                width: parent.width
                                spacing: 3

                                Repeater {
                                    model: movement.g.rows
                                    delegate: Column {
                                        id: bay
                                        required property var modelData
                                        readonly property var row: modelData
                                        width: playbill.width
                                        spacing: 3

                                        SessionCard { s: bay.row.s; hue: movement.hue }

                                        Repeater {
                                            model: bay.row.kids
                                            delegate: Item {
                                                required property var modelData
                                                width: bay.width
                                                implicitHeight: kidCard.implicitHeight

                                                Text {   // the hanger, in the gutter
                                                    x: 7; y: 2
                                                    text: "└"
                                                    font.family: temple.faceMono
                                                    font.pixelSize: 11
                                                    color: temple.withA(temple.livery.wireCyan, 0.5)
                                                }
                                                SessionCard {
                                                    id: kidCard
                                                    anchors.left: parent.left
                                                    anchors.leftMargin: 18
                                                    anchors.right: parent.right
                                                    s: modelData
                                                    child: true
                                                    hue: movement.hue
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // slim gold scrollbar in the body gutter — draggable (ScrollRail); the
        // rail sits in the stele's 13px margin, clear of the playbill, so its
        // grab lane never covers a session card.
        ScrollRail {
            flick: flick
            railW: 3
            minThumb: 20
            trackColor: temple.withA(livery.paletteFg, 0.10)
            thumbColor: temple.withA(temple.signature, 0.75)
            anchors.top: flick.top; anchors.bottom: flick.bottom
            anchors.right: parent.right; anchors.rightMargin: 9
        }

        // ── STYLOBATE: └─┤ tally ├──── 𝄂 ┘ ───────────────────────────────────
        Item {
            id: foot
            anchors.left: parent.left; anchors.right: parent.right
            anchors.bottom: parent.bottom
            anchors.margins: 13
            height: 18

            Text {
                id: ffL
                anchors.left: parent.left
                // right-capped at the final barline (acyclic: ffBar is fixed)
                // so 30+ voices can never push the tally under the chrome
                anchors.right: ffBar.left; anchors.rightMargin: 8
                anchors.verticalCenter: parent.verticalCenter
                elide: Text.ElideRight
                text: "└─┤ " + (temple.totalCount === 0
                                ? "tacet"
                                : (temple.totalCount + " voices · "
                                   + temple.workingCount + " working · "
                                   + temple.livery.ctxCompact(temple.totalTokens)
                                   + " tok · ϟ" + temple.hookedCount)) + " ├"
                font.family: temple.faceMono; font.pixelSize: 11
                color: temple.withA(livery.paletteFg, 0.8)
            }
            Text {
                id: ffCorner
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                text: "┘"
                font.family: temple.faceMono; font.pixelSize: 11
                color: temple.withA(temple.signature, 0.95)
            }
            Text {                               // the final barline closes the score
                id: ffBar
                anchors.right: ffCorner.left; anchors.rightMargin: 4
                anchors.verticalCenter: parent.verticalCenter
                anchors.verticalCenterOffset: -1
                text: "𝄂"
                font.family: temple.faceMusic; font.pixelSize: 16
                color: livery.paletteAccent
            }
            Rectangle {
                anchors.left: ffL.right; anchors.right: ffBar.left
                anchors.leftMargin: 2; anchors.rightMargin: 2
                anchors.verticalCenter: parent.verticalCenter
                anchors.verticalCenterOffset: 1
                height: 1
                color: temple.withA(temple.signature, 0.55)
            }
        }
    }

    // ═════ ONE PLAQUE ══════════════════════════════════════════════════════════
    // A hairline-bordered card, ONE SURFACE — not zones. The plaque fill/
    // border and the left pilaster are the card's only chrome; no rectangle
    // scopes a sub-region beneath them. Six rows, crest to ground:
    //
    // 1 · IDENTITY — lamp · agent name (s.agent VERBATIM, no lookup — the
    // autohook) · ⟐/⇄ kind + ϟ hook tags, and the CATALOGUE CORNER reading
    // right→left: the #NN graph badge outermost (rebuild()'s ordinal — #NN
    // main, #NN.k sub — rendered here, never assigned), wsN inside it
    // (mains only).
    //
    // 2 · PROVENANCE — harness/model beside petname and real ID suffix.
    // Expandable details preserve the complete ID and copyable recovery data.
    //
    // 3 · DIRECTIVE — explicit prompt content, independent of the title.
    //
    // 4 · VOICE: thinking — a FIXED box (4 lines main / 2 sub, full width):
    // text wraps, the LAST line elides, tool text popping in never shifts
    // the card. sudo — the one urgent flag — pins to the lane's own top-
    // right corner when held.
    //
    // 4½ · SUMMONS — a 20px lane that EXISTS only while a permission summons
    // for this session stands in the herald ledger: the ask (the summons
    // summary) left, approve / deny chips right. The one place the card's
    // silhouette moves automatically; details expand only on request. Same wire line and same daemon guards as the herald's card.
    //
    // 5 · PULSE — ctx (this session's OWN context window, NUMBERS ONLY —
    // no percentage text, no bar glyph; ctxColor severity rides the colour
    // alone; mains only) and up (elapsed since start, both kinds) SHARE one
    // line: ctx left, up right-anchored beside it on mains (the same read-
    // right-to-left corner idiom as the #NN/wsN pair in row 1); subs (no
    // ctx to share with) left-anchor up alone at the row's usual margin.
    //
    // 6 · GROUND — mains close with cwd (ElideLeft keeps the tail) beside
    // the clipped troupe box — the pairing idiom rows 2 and 5 both copy;
    // subs close with the troupe alone. Unmoved.
    //
    // Closed details and fixed activity lanes keep a stable silhouette, no
    // jitter as live data streams in; the animated elements ride reserved
    // boxes (lamp 16px, troupe 116px main / 90px sub) so nothing floats.
    // No zone-break spacers, no card-level dividers — one Column rhythm,
    // one container. radius 0; colour only from temple.livery.* roles.
    component SessionCard: Item {
        id: card

        property var s: ({})
        property bool child: false

        property color hue: temple.signature

        width: parent ? parent.width : 0
        implicitHeight: inner.implicitHeight + 11   // 5 top + text + 6 base

        readonly property string sState: (s && s.state) ? ("" + s.state).toLowerCase() : ""
        readonly property string hookPh: temple.hookPhase(s ? s.sessionId : "")
        // THE LIVE STATE — the hook phase wins when present; the roster state
        // is the fallback. This drives the lamp glyph/colour, the metronome
        // pulse, the terracotta breath, the border, and the kaomoji face.
        // Laurel/firstWorkingId/workingCount stay ROSTER-based (stable tallies).
        readonly property string cardLiveState: hookPh !== "" ? hookPh : sState
        readonly property bool sudoHeld: !!(s && s.needsSudo)
        // the standing permission summons for THIS session, or null — the whole
        // condition for drawing the verdict chips (see the SUMMONS channel
        // above; the ledger, not `awaiting`, is what proves it is answerable).
        readonly property var summonsRec:
            (s && s.sessionId) ? (temple.summonsById[s.sessionId] || null) : null
        readonly property bool summoned: !!card.summonsRec
        readonly property bool cardWorking: cardLiveState === "working"
        readonly property bool cardAwaiting: cardLiveState === "awaiting" || sudoHeld
        readonly property bool cardResting: !cardWorking && !cardAwaiting
        readonly property bool hasCtx: !!(s && s.contextTokens > 0)
        readonly property real ctxPct: hasCtx
            ? temple.livery.ctxPercent(s.contextTokens, s.contextCeiling) : 0
        readonly property bool laurel:
            temple.emphasizedId !== "" && !!s && s.sessionId === temple.emphasizedId
        readonly property string sKind: temple.kindOf(s)
        readonly property bool hasWs:
            !!(s && s.workspace !== undefined && s.workspace !== null)
        readonly property bool hooked: temple.hooked(s ? s.sessionId : "")
        // the pi-harness badge — a MAIN pi session swaps the ϟ bolt for its
        // own π think-tag (piTag, below). The tag EXISTS only while the live
        // state is working (the self-writing loop) — at rest it vanishes
        // entirely, no still π and no ϟ. Subs keep the ϟ. NOT hook-gated
        // (was card.hooked &&): kimi's moon fires on plain working with no
        // hook, and an unhooked working pi never got its animation at all —
        // the think-tags share one grammar now: working MAIN of that agent.
        readonly property bool piThinking: !card.child
            && !!card.s && !!card.s.agent
            && ("" + card.s.agent).toLowerCase() === "pi"
        readonly property bool piLive: card.piThinking && card.cardWorking
        // the kimi moon — a MAIN kimi session wears kimi-code's own thinking
        // animation (the moon spinner) right of its name, only for the spin:
        // nothing is reserved at rest (moonTag below).
        readonly property bool kimiMain: !card.child && !!card.s && !!card.s.agent
            && ("" + card.s.agent).toLowerCase() === "kimi"
        // Paint identity comes from the record, never the window title or model.
        readonly property bool codexMain: !card.child && card.sKind === "agent"
            && card.agentName.toLowerCase() === "codex"
        readonly property bool codexLive: card.codexMain && card.cardWorking
            && !card.cardAwaiting

        // troupe casting — hashed per session id, couriers from the packages
        // pool, everyone else from the general pool; resting states hold the
        // shared still pose for their (live) state.
        readonly property var facePool:
            sKind === "subagent" ? faces.packages : faces.working
        readonly property int faceSet:
            faces.pickFor((s && s.sessionId) || "", facePool)
        readonly property var faceFrames: faces.workingFrames(faceSet, facePool)
        readonly property string faceText: cardWorking
            ? faceFrames[(temple.faceTick
                          + faces.phaseFor((s && s.sessionId) || "", faceFrames.length))
                         % faceFrames.length]
            : faces.still(cardLiveState, true)

        // the thinking lane — the agent's WORDS, else the hook placeholder. The
        // tool used to share this box (a "▸ tool" first line above the say);
        // it has its own row now (§3.5), so a long say gets all four lines back
        // and a tool popping in no longer pushes the words out of view.
        readonly property string sayText: (s && s.say)
            ? ("" + s.say).replace(/\s+/g, " ") : ""
        readonly property bool thinkSaid: sayText !== ""
        readonly property string thinkText: thinkSaid
            ? sayText
            : (card.hooked ? ("ϟ " + card.cardLiveState) : "…")

        // the tool lane — what the agent last reached for. TWO sources, and
        // they disagree on purpose: `activity` is hook-set the instant a tool
        // starts but is only ever its bare NAME ("Bash"), and is cleared when
        // the turn settles; `tool` is read off the transcript by the reaper, so
        // it carries the subject ("Bash: cargo test") and SURVIVES the settle,
        // but can sit one reap (~12s) behind. Same call → take the rich label;
        // a live tool the transcript hasn't caught up to → take the live name.
        readonly property string liveTool: (s && s.activity) ? ("" + s.activity) : ""
        readonly property string lastTool: (s && s.tool) ? ("" + s.tool) : ""
        readonly property string toolText: {
            if (liveTool === "") return lastTool
            var a = liveTool.toLowerCase(), b = lastTool.toLowerCase()
            return (b === a || b.indexOf(a + ":") === 0) ? lastTool : liveTool
        }

        // ── the identity trinity — graph number · session id · agent name ────
        // noBadge reads s.no verbatim, stamped in rebuild(): a top-level int
        // (zero-padded, #01) for a main, or a "parent.k" STRING (#01.2) for a
        // subagent — it never claims a top-level slot of its own.
        readonly property string noBadge: {
            if (!s || s.no === undefined || s.no === null) return "#—"
            if (card.child) return "#" + s.no
            return "#" + (s.no < 10 ? "0" + s.no : s.no)
        }
        // the agent's OWN name, verbatim — no lookup, no special-casing, so an
        // agent nobody has registered yet still renders correctly (autohook).
        readonly property string agentName: (s && s.agent) ? ("" + s.agent) : "agent"
        // Petname and real ID suffix are display hints; routing keeps the full ID.
        readonly property string idCallout: {
            if (s && s.petname && s.petname !== "") {
                var sid = (s.sessionId || "")
                if (sid && sid.length > 0) {
                    var suffix = sid.length > 4 ? ("…" + sid.slice(-4)) : sid
                    return ("" + s.petname) + " · " + suffix
                }
                return ("" + s.petname)
            }
            var v = (s && s.sessionId) ? ("" + s.sessionId) : ""
            return v.length > 4 ? "…" + v.slice(-4) : v
        }
        // A session title and an actual prompt are separate published fields.
        readonly property string promptText: (s && s.prompt)
            ? ("" + s.prompt).replace(/\s+/g, " ") : ""
        // Missing titles fall back to the harness, never the petname.
        readonly property string displayName: {
            if (s && s.title && ("" + s.title).trim() !== "")
                return ("" + s.title).replace(/\s+/g, " ").trim()
            return card.agentName
        }

        // the plaque ground + hairline (terracotta while awaiting)
        Rectangle {
            anchors.fill: parent
            radius: 0
            color: cardMouse.containsMouse
                   ? temple.withA(temple.signature, 0.10)
                   : (card.laurel ? temple.withA(temple.livery.paletteHot, 0.05)
                                  : temple.withA(temple.livery.paletteFg, 0.035))
            border.width: 1
            border.color: card.cardAwaiting
                          ? temple.withA(temple.livery.paletteUrgent, 0.75)
                          : temple.withA(temple.livery.paletteFg, 0.22)
        }
        // the project pilaster — laurel when this is the one traced plaque.
        // The crown widens to the pantheon's 3px (Terminals' crown bar is 3);
        // the resting pilaster keeps its slim 2px project-hue stripe.
        Rectangle {
            x: 0; width: card.laurel ? 3 : 2
            height: parent.height
            color: card.laurel ? temple.livery.paletteHot
                               : temple.withA(card.hue, 0.6)
        }

        Column {
            id: inner
            anchors.left: parent.left; anchors.leftMargin: 9
            anchors.right: parent.right; anchors.rightMargin: 6
            anchors.top: parent.top; anchors.topMargin: 5
            spacing: 3
            // ABOVE the card-wide click area (cardMouse, declared after this
            // Column). Only the verdict chips need it — plain Text/Rectangle
            // never accept a mouse event, so hover, trace and click-to-focus
            // still reach cardMouse everywhere else on the plaque. Without it
            // the chips are buried and every press focuses the window instead.
            z: 1

            // ── 1 · IDENTITY — lamp · name · tags ── wsN · #NN ────────────────
            // The inscription. The graph badge holds the catalogue corner,
            // outermost, so the name owns the whole left run.
            Item {
                width: parent.width
                height: card.child ? 16 : 20

                Text {                           // #NN — the catalogue corner
                    id: badgeT
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    text: card.noBadge
                    font.family: temple.faceMono; font.pixelSize: 10
                    font.weight: Font.DemiBold
                    color: card.laurel ? temple.livery.paletteHot
                                       : temple.withA(temple.signature, 0.95)
                }
                Text {                           // workspace tag — inside the corner
                    id: wsT
                    visible: !card.child && card.hasWs
                    anchors.right: badgeT.left; anchors.rightMargin: 8
                    anchors.verticalCenter: parent.verticalCenter
                    text: card.hasWs ? ("ws" + card.s.workspace) : ""
                    font.family: temple.faceMono; font.pixelSize: 9
                    // the bar's own note hue for this workspace — the same
                    // colour grammar the Terminals ground tag already speaks
                    // (noteColor is safe for id <= 0)
                    color: temple.withA(temple.livery.noteColor(card.hasWs ? card.s.workspace : 0), 0.9)
                }
                Text {                           // state word — the lamp's caption,
                                                 // same live state + colour source as
                                                 // the glyph so the two never disagree;
                                                 // always rendered ("—" fallback) so the
                                                 // corner slot never jitters.
                    id: stateWordT
                    anchors.right: wsT.visible ? wsT.left : badgeT.left
                    anchors.rightMargin: 8
                    anchors.verticalCenter: parent.verticalCenter
                    text: card.cardLiveState !== "" ? card.cardLiveState : "—"
                    font.family: temple.faceSerif; font.italic: true
                    font.pixelSize: 10   // the family state-word size (Terminals matches)
                    color: temple.withA(temple.lampColor(card.cardLiveState),
                                        card.cardResting ? 0.55 : 1.0)
                }

                Item {                           // the reserved lamp box
                    id: lampBox
                    x: 0
                    anchors.verticalCenter: parent.verticalCenter
                    width: 16; height: 16

                    Text {
                        id: lamp
                        anchors.centerIn: parent
                        // sudo swaps the lamp for the SAME nf-fa-lock glyph the
                        // Terminals badge wears (written as an escape — the raw
                        // PUA char is invisible in editors and got lost once,
                        // leaving this branch an empty string).
                        text: card.sudoHeld ? "\uf023" : temple.lampGlyph(card.cardLiveState)
                        font.family: card.sudoHeld ? temple.faceMono : temple.faceMusic
                        font.pixelSize: 12
                        color: card.laurel
                               ? temple.livery.paletteHot
                               : temple.withA(card.sudoHeld ? temple.livery.paletteUrgent
                                                            : temple.lampColor(card.cardLiveState),
                                              card.cardResting ? 0.55 : 1.0)

                        // working — a metronome pulse, confined to the box
                        SequentialAnimation on scale {
                            running: card.cardWorking
                            loops: Animation.Infinite
                            alwaysRunToEnd: true
                            NumberAnimation { to: 1.35; duration: 520; easing.type: Easing.InOutSine }
                            NumberAnimation { to: 1.0;  duration: 520; easing.type: Easing.InOutSine }
                        }
                        // awaiting — a terracotta breath, box-neutral. A sudo
                        // hold quickens it to the Terminals badge's 380ms ping
                        // (one urgency cadence across both temples: "your
                        // password" beats faster than "an agent question").
                        SequentialAnimation on opacity {
                            running: card.cardAwaiting
                            loops: Animation.Infinite
                            alwaysRunToEnd: true
                            NumberAnimation { to: 0.35; duration: card.sudoHeld ? 380 : 700; easing.type: Easing.InOutSine }
                            NumberAnimation { to: 1.0;  duration: card.sudoHeld ? 380 : 700; easing.type: Easing.InOutSine }
                        }
                    }
                }

                Text {                           // agent name — the card's name
                    id: nameT
                    // Every harness keeps the same reserved animation lane at rest.
                    anchors.left: lampBox.right
                    anchors.leftMargin: 35
                    anchors.verticalCenter: parent.verticalCenter
                    text: card.displayName
                    elide: Text.ElideRight
                    width: Math.min(implicitWidth,
                                    parent.width - 51 - badgeT.implicitWidth - 10
                                    - (kindTag.visible ? kindTag.implicitWidth + 6 : 0)
                                    - (wsT.visible ? wsT.implicitWidth + 8 : 0)
                                    - (stateWordT.implicitWidth + 8))
                    font.family: temple.faceSerif
                    // 14 on mains — the family name size (Terminals' main
                    // label is serif/mono 14); subs keep their smaller step.
                    font.pixelSize: card.child ? 12 : 14
                    // the traced plaque bolds its name, same as the Terminals
                    // emph row already does.
                    font.weight: card.laurel ? Font.Bold : Font.Medium
                    color: temple.withA(temple.livery.paletteFg, card.child ? 0.85 : 1.0)
                }
                Text {                           // kind tag — WITH the name
                    id: kindTag
                    visible: card.sKind === "subagent"
                    anchors.left: nameT.right; anchors.leftMargin: 6
                    anchors.verticalCenter: parent.verticalCenter
                    text: "⟐ sub"
                    font.family: temple.faceMono; font.pixelSize: 9
                    color: temple.livery.violet
                }
                Text {                           // the ϟ hook tag — the pi
                                                 // harness wears the π think-
                                                 // tag instead (piTag below),
                                                 // a kimi main the moon,
                                                 // a codex main the book
                    id: hookTag
                    visible: card.hooked && card.cardWorking && !card.piThinking && !card.kimiMain && !card.codexMain
                    anchors.left: lampBox.right
                    anchors.leftMargin: 6
                    anchors.verticalCenter: parent.verticalCenter
                    anchors.verticalCenterOffset: 2
                    // WORKING + claude + main → cycles the same reverse-
                    // engineered spinner glyphs/timing as UsageGadget's ❋
                    // (hazards §1: already live-verified at this face,
                    // "Noto Sans Symbols 2" — new size/context here, 9px
                    // inline vs 26px standalone spark, so still live-checked
                    // rather than assumed). NOT reusing UsageGadget's
                    // glyphYOffset/wobble table — that was calibrated for a
                    // spark centred against its own wordmark sibling, a
                    // different enough context; this tag keeps its plain
                    // verticalCenter, unchanged. Duplicated rather than
                    // extracted into a shared component: the part that IS
                    // shared (the glyph array + hold-first/last timing) is
                    // small, the part that would NOT transfer (Y-correction)
                    // is the bigger half of the source, and a new shared
                    // component reopens hazards §3's same-directory dynamic-
                    // resolution risk for two call sites. Flag: a third call
                    // site would tip this toward extraction.
                    readonly property bool spinning: card.cardWorking && !card.child
                        && card.agentName.toLowerCase() === "claude"
                    readonly property var spinGlyphs: ["·", "✻", "✽", "✶", "✳", "✢"]
                    // guarded index, not cosmetic: triggeredOnStart's first
                    // fire lands on the NEXT event-loop tick, not the same
                    // one as `spinning` flipping true, so this binding does
                    // see frame===-1 transiently — spinGlyphs[-1] is
                    // undefined, which QML logs assigning to a QString.
                    // UsageGadget's own spinner sidesteps this by writing
                    // clef.text imperatively inside onTriggered instead of
                    // binding it reactively; kept this one declarative and
                    // just guarded the fallback, rather than also moving the
                    // non-spinning "ϟ" branch off a ternary to match exactly
                    // (hazards §3: a property both bound and written breaks
                    // on first write, so that switch is not a small one).
                    text: hookTag.spinning ? (hookTag.spinGlyphs[hookSpin.frame] || "ϟ") : "ϟ"
                    // "·" (U+00B7 MIDDLE DOT) isn't in Noto Sans Symbols 2 —
                    // same gap UsageGadget's clef hit at 26px, just quieter
                    // here at 10px rather than actually invisible. Same fix:
                    // that one frame reads from faceMono regardless of
                    // spinning state (resting "ϟ" was already faceMono).
                    font.family: (hookTag.spinning && hookTag.text !== "·") ? temple.faceSymbol : temple.faceMono
                    font.pixelSize: 10   // up from 9 (pixelSize is int — 9.45 isn't valid) — the User: "a bit bigger"
                    color: temple.withA(temple.lampColor(card.cardLiveState), 0.95)

                    Timer {
                        id: hookSpin
                        interval: baseIntervalMs
                        repeat: true
                        triggeredOnStart: true
                        running: hookTag.spinning
                        property int frame: -1
                        readonly property int baseIntervalMs: 170
                        readonly property int holdIntervalMs: 300
                        onTriggered: {
                            frame = (frame + 1) % hookTag.spinGlyphs.length
                            interval = (frame === 0 || frame === hookTag.spinGlyphs.length - 1)
                                       ? holdIntervalMs : baseIntervalMs
                        }
                    }
                }

                // the π think-tag — the pi harness's OWN hook badge. Where a
                // hooked claude main cycles its glyphs, a hooked pi main
                // swaps the ϟ bolt for a slow SELF-WRITING π: left stem
                // draws, right stem draws, then the top bar sweeps across —
                // a long still hold — then the pen gently unwrites and the
                // page rests blank for a beat. A still, deliberate loop
                // (draw ≈1.5s, hold ≈1.1s, unwrite ≈0.75s, rest ≈0.5s):
                // pen-on-page thinking, not a spinner. The tag exists ONLY
                // while the live state is working — at rest it vanishes
                // entirely (no still π, no ϟ). Same fixed 13×16 reserved
                // box, same anchoring as the ϟ tag it replaces — nothing
                // floats, no layout shift.
                Item {
                    id: piTag
                    visible: card.piLive
                    anchors.left: lampBox.right
                    anchors.leftMargin: 6
                    anchors.verticalCenter: parent.verticalCenter
                    anchors.verticalCenterOffset: 1   // rests a tick below the name's optical centre
                    width: 13; height: 16

                    // the pen's progress: 0 = blank page, 1 = full π. Starts
                    // at 0 so a fresh card writes itself in on appearance.
                    property real drawProgress: 0
                    SequentialAnimation on drawProgress {
                        running: card.piLive
                        loops: Animation.Infinite
                        alwaysRunToEnd: true
                        NumberAnimation { to: 1; duration: 1500; easing.type: Easing.InOutQuad }
                        PauseAnimation { duration: 1100 }
                        NumberAnimation { to: 0; duration: 750; easing.type: Easing.InOutQuad }
                        PauseAnimation { duration: 500 }
                    }

                    Canvas {
                        anchors.fill: parent
                        property real p: piTag.drawProgress
                        onPChanged: requestPaint()
                        onWidthChanged: requestPaint()
                        onHeightChanged: requestPaint()
                        onPaint: {
                            var ctx = getContext("2d")
                            ctx.reset()
                            var w = width, h = height
                            ctx.strokeStyle = temple.withA(temple.lampColor(card.cardLiveState), 0.95)
                            ctx.lineWidth = 1.6
                            ctx.lineCap = "round"
                            // pen geometry — the top bar rides a fifth of
                            // the way down; the stems start just beneath it
                            // and run to ~3/4 of the height (short legs, ~2px
                            // above the box's floor), so the round caps
                            // overlap the bar and the joints read as one
                            // continuous glyph, never three broken strokes.
                            var barY = h * 0.2
                            var stemTop = barY + 0.6
                            var stemBot = h * 0.72
                            var xL = w * 0.27, xR = w * 0.73
                            var barL = w * 0.17, barR = w * 0.83
                            function ease(t) {
                                return t < 0.5 ? 2 * t * t
                                               : 1 - Math.pow(-2 * t + 2, 2) / 2
                            }
                            function seg(p0, p1, ax, ay, bx, by) {
                                var f = (p - p0) / (p1 - p0)
                                if (f <= 0) return
                                if (f > 1) f = 1
                                f = ease(f)
                                ctx.moveTo(ax, ay)
                                ctx.lineTo(ax + (bx - ax) * f, ay + (by - ay) * f)
                            }
                            ctx.beginPath()
                            seg(0.00, 0.30, xL, stemTop, xL, stemBot)  // left stem
                            seg(0.30, 0.62, xR, stemTop, xR, stemBot)  // right stem
                            seg(0.62, 1.00, barL, barY, barR, barY)    // top bar
                            ctx.stroke()
                        }
                    }
                }

                // Codex's illuminated book — a printed leaf turns while working,
                // a still open spread on a hold, and a closed cover at rest.
                // Both leaf faces share the settled page's text and geometry;
                // the loop joins on the same fully printed spread.
                Item {
                    id: codexTag
                    visible: card.codexMain
                    anchors.left: lampBox.right; anchors.leftMargin: 6
                    anchors.verticalCenter: parent.verticalCenter
                    width: 24; height: 18
                    clip: true
                    readonly property color ink: temple.withA(temple.lampColor(card.cardLiveState), 0.95)
                    readonly property color gold: temple.livery.paletteAccent
                    readonly property color paper: temple.livery.paletteBg
                    readonly property bool openBook: card.cardWorking || card.cardAwaiting
                    readonly property bool turning: card.codexLive
                    readonly property var hostWindow: QsWindow.window
                    property var hostTransforms: []
                    property bool componentReady: false
                    Component.onCompleted: {
                        hostTransforms = temple.markTransforms(codexTag)
                        componentReady = true
                    }
                    Component.onDestruction: componentReady = false
                    // Attachment signals fire before the outer root id is available.
                    onParentChanged: if (componentReady && temple) hostTransforms = temple.markTransforms(codexTag)
                    onHostWindowChanged: if (componentReady && temple) hostTransforms = temple.markTransforms(codexTag)
                    readonly property bool exposed: componentReady && !!temple
                        && temple.markExposed(codexTag, hostWindow, hostTransforms)
                    property real leaf: 0

                    SequentialAnimation on leaf {
                        running: codexTag.turning && codexTag.exposed
                        loops: Animation.Infinite
                        onStopped: codexTag.leaf = 0
                        NumberAnimation { from: 0; to: 1; duration: 1100; easing.type: Easing.InOutCubic }
                        PauseAnimation { duration: 450 }
                    }
                    Canvas {
                        anchors.fill: parent
                        property color ink: codexTag.ink
                        property color gold: codexTag.gold
                        property color paper: codexTag.paper
                        property bool openBook: codexTag.openBook
                        property bool turning: codexTag.turning
                        property real leaf: codexTag.leaf
                        onInkChanged: requestPaint()
                        onGoldChanged: requestPaint()
                        onPaperChanged: requestPaint()
                        onOpenBookChanged: requestPaint()
                        onTurningChanged: requestPaint()
                        onLeafChanged: requestPaint()
                        onWidthChanged: requestPaint()
                        onHeightChanged: requestPaint()
                        onPaint: {
                            var ctx = getContext("2d")
                            ctx.reset()
                            ctx.strokeStyle = ink
                            ctx.fillStyle = paper
                            ctx.lineWidth = 1
                            if (!openBook) {
                                // The resting volume: hard cover, spine, page block,
                                // and one small gilt lozenge, all inside the same slot.
                                ctx.globalAlpha = 0.8
                                ctx.fillRect(7, 2.5, 11, 13)
                                ctx.strokeRect(7, 2.5, 11, 13)
                                ctx.beginPath()
                                ctx.moveTo(9, 2.5); ctx.lineTo(9, 15.5)
                                ctx.moveTo(9, 13.5); ctx.lineTo(18, 13.5)
                                ctx.stroke()
                                ctx.strokeStyle = gold
                                ctx.globalAlpha = 0.55
                                ctx.beginPath()
                                ctx.moveTo(13.5, 6); ctx.lineTo(15, 8)
                                ctx.lineTo(13.5, 10); ctx.lineTo(12, 8)
                                ctx.closePath(); ctx.stroke()
                                return
                            }

                            // Every face uses this painter: the same outline and printed
                            // strokes at rest, on the front of a leaf, and on its back.
                            // The gutter is excluded here and stroked once after all pages.
                            function page(side, lift, alpha) {
                                var edgeX = 12 + 10 * side
                                ctx.strokeStyle = ink
                                ctx.lineWidth = 1
                                ctx.globalAlpha = alpha
                                ctx.beginPath()
                                ctx.moveTo(12, 5)
                                ctx.quadraticCurveTo(12 + 5 * side, 2 - 2 * lift,
                                                     edgeX, 3.5 - 1.5 * lift)
                                ctx.lineTo(edgeX, 14 - lift)
                                ctx.quadraticCurveTo(12 + 5 * side, 12.5 - lift, 12, 15)
                                ctx.fill()   // fill closes the gutter; the outline does not
                                ctx.globalAlpha = 0.8 * alpha
                                ctx.stroke()
                                // Identical decorative text on both faces. Foreshortening
                                // compresses the lines into the gutter as the page turns.
                                ctx.globalAlpha = 0.3 * alpha * Math.abs(side)
                                for (var row = 0; row < 3; row++) {
                                    var y = 6 + row * 2.5
                                    ctx.beginPath()
                                    ctx.moveTo(12 + 3 * side, y + 0.7 - 0.6 * lift)
                                    ctx.lineTo(12 + 8 * side, y - 1.6 * lift)
                                    ctx.stroke()
                                }
                            }
                            page(-1, 0, 1)
                            page(1, 0, 1)

                            if (turning && leaf > 0 && leaf < 1) {
                                var angle = Math.PI * leaf
                                var lift = Math.sin(angle)
                                var side = Math.cos(angle)
                                // Vanishing overlay weight at BOTH endpoints prevents a
                                // translucent outline being double-painted at landing/reset.
                                // Geometry and text also converge to the resting page exactly.
                                var alpha = lift * lift
                                page(side, lift, alpha)
                                ctx.strokeStyle = gold
                                ctx.beginPath()
                                ctx.moveTo(12 + 10 * side, 3.5 - 1.5 * lift)
                                ctx.lineTo(12 + 10 * side, 14 - lift)
                                ctx.globalAlpha = 0.13 * lift * alpha
                                ctx.lineWidth = 3; ctx.stroke()
                                ctx.globalAlpha = 0.55 * lift * alpha
                                ctx.lineWidth = 1; ctx.stroke()
                            }

                            // One shared gutter and cover edge, independent of the leaf.
                            // The landed spread and the next turn's start are the same image.
                            ctx.strokeStyle = ink
                            ctx.globalAlpha = 0.8
                            ctx.lineWidth = 1
                            ctx.beginPath()
                            ctx.moveTo(12, 5); ctx.lineTo(12, 15)
                            ctx.moveTo(1.5, 15); ctx.lineTo(7, 14.5)
                            ctx.lineTo(12, 16); ctx.lineTo(17, 14.5)
                            ctx.lineTo(22.5, 15)
                            ctx.stroke()
                        }
                    }
                }

                Text {                           // the moon think-tag — a kimi MAIN
                                                 // wears kimi-code's own moon
                                                 // spinner while working
                    id: moonTag
                    visible: card.kimiMain && card.cardWorking
                    anchors.left: lampBox.right
                    anchors.leftMargin: 6
                    anchors.verticalCenter: parent.verticalCenter
                    // frames + 120ms cadence lifted verbatim from kimi-code's
                    // MOON_SPINNER (tui/constant/rendering.ts): 8 phases, one
                    // full lunation ≈ 1s, uncoloured — the emoji face carries
                    // its own colour. Frame starts at -1 with a guarded index
                    // for the same transient as hookTag (triggeredOnStart's
                    // first fire lands on the next tick, not this one).
                    readonly property var moonGlyphs: ["🌑", "🌒", "🌓", "🌔", "🌕", "🌖", "🌗", "🌘"]
                    text: moonTag.moonGlyphs[moonSpin.frame] || "🌑"
                    font.family: temple.faceEmoji
                    font.pixelSize: 10

                    Timer {
                        id: moonSpin
                        interval: 120
                        repeat: true
                        triggeredOnStart: true
                        running: moonTag.visible
                        property int frame: -1
                        onTriggered: frame = (frame + 1) % moonTag.moonGlyphs.length
                    }
                }
            }

            Item {
                id: identityRow
                width: parent.width
                readonly property bool split: width < 300 && harnessIdentity.visible
                height: split ? 28 : 14
                readonly property real availableWidth: width
                Text {
                    id: harnessIdentity
                    anchors.left: parent.left
                    visible: !!(card.s && card.s.model) || card.displayName !== card.agentName
                    width: !visible ? 0 : (identityRow.split ? parent.width
                        : Math.max(0, identityRow.availableWidth - Math.max(96, identityRow.availableWidth * 0.55) - 8))
                    text: card.agentName + ((card.s && card.s.model) ? " / " + card.s.model : "")
                    elide: Text.ElideMiddle
                    font.family: temple.faceMono; font.pixelSize: 10
                    color: temple.withA(temple.livery.paletteFg, 0.65)
                }
                Text {
                    anchors.left: harnessIdentity.visible && !identityRow.split ? harnessIdentity.right : parent.left
                    anchors.leftMargin: harnessIdentity.visible && !identityRow.split ? 8 : 0
                    y: identityRow.split ? 14 : 0
                    anchors.right: parent.right
                    anchors.rightMargin: 0
                    horizontalAlignment: Text.AlignRight
                    text: card.idCallout
                    elide: Text.ElideMiddle
                    font.family: temple.faceMono; font.pixelSize: 10
                    color: temple.withA(temple.livery.paletteFg, 0.65)
                }

            }

            // Prompt content has its own field; a title never substitutes for it.
            Item {
                width: parent.width
                visible: card.promptText !== ""
                height: visible ? 14 : 0

                Text {
                    id: promptCaret
                    visible: card.promptText !== ""
                    anchors.left: parent.left; anchors.leftMargin: 21
                    anchors.verticalCenter: parent.verticalCenter
                    text: "»"
                    font.family: temple.faceMono; font.pixelSize: 11
                    color: temple.withA(temple.signature, 0.8)
                }
                Text {
                    anchors.left: promptCaret.visible ? promptCaret.right : parent.left
                    anchors.leftMargin: promptCaret.visible ? 6 : 21
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    text: card.promptText !== "" ? card.promptText : "—"
                    elide: Text.ElideRight
                    font.family: temple.faceMono; font.pixelSize: 10
                    color: temple.withA(temple.livery.paletteFg,
                                        card.promptText !== "" ? 0.62 : 0.28)
                }
            }

            // ── 3.5 · HAND: the tool — one line, ▸ caret, its own slot ───────
            // Between the directive it was given and the words it answers with:
            // what the agent is REACHING FOR. Always rendered ("—" holds the
            // slot, same idiom as the directive above), so a tool arriving or
            // clearing never resizes the plaque and never displaces the say.
            // Dimmed at rest — then it reads as the last thing done, not a
            // claim that something is running.
            Item {
                width: parent.width
                height: 14

                Text {
                    id: toolCaret
                    anchors.left: parent.left; anchors.leftMargin: 21
                    anchors.verticalCenter: parent.verticalCenter
                    text: "▸"
                    font.family: temple.faceMono; font.pixelSize: 10
                    color: temple.withA(temple.signature,
                                        card.toolText !== ""
                                        ? (card.cardWorking ? 0.85 : 0.45) : 0.25)
                }
                Text {
                    anchors.left: toolCaret.right; anchors.leftMargin: 6
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    text: card.toolText !== "" ? card.toolText : "—"
                    elide: Text.ElideRight
                    font.family: temple.faceMono; font.pixelSize: 10
                    color: temple.withA(temple.livery.paletteFg,
                                        card.toolText !== ""
                                        ? (card.cardWorking ? 0.7 : 0.45) : 0.28)
                }
            }

            // ── 4 · VOICE: thinking — the agent's own words, full width. A
            // FIXED box (4 lines main / 2 sub): text wraps, the LAST line
            // elides, a longer say never shifts the card. sudo — the one
            // urgent flag — pins to the lane's own top-right corner when held.
            Item {
                width: parent.width
                height: card.child ? 22 : 44

                Text {                           // sudo — top-right corner
                    id: sudoT
                    visible: card.sudoHeld
                    anchors.top: parent.top
                    anchors.right: parent.right
                    text: "sudo"
                    font.family: temple.faceMono; font.pixelSize: 9
                    color: temple.livery.paletteUrgent
                }
                Text {                           // the thinking — wraps, last-
                    id: thinkT                     // line elide, fixed lane
                    anchors.left: parent.left; anchors.leftMargin: 21
                    anchors.right: sudoT.visible ? sudoT.left : parent.right
                    anchors.rightMargin: sudoT.visible ? 6 : 0
                    anchors.top: parent.top
                    height: parent.height
                    verticalAlignment: Text.AlignTop
                    wrapMode: Text.WordWrap
                    elide: Text.ElideRight
                    lineHeight: 10
                    lineHeightMode: Text.FixedHeight
                    text: card.thinkText
                    // the ϟ placeholder is machine text (mono, upright); the
                    // agent's own words stay serif italic — the voice.
                    font.family: (!card.thinkSaid && card.hooked)
                                 ? temple.faceMono : temple.faceSerif
                    font.italic: card.thinkSaid || !card.hooked
                    font.pixelSize: 10
                    color: (card.thinkSaid || card.hooked)
                           ? temple.withA(temple.livery.paletteFg, 0.55)
                           : temple.withA(temple.livery.paletteFg, 0.25)
                }
            }

            // ── 4½ · SUMMONS — the permission gate, answered in place ────────
            // Present ONLY while a summons for this session stands in the
            // herald ledger; the card grows by this one 20px lane and shrinks
            // back the moment a verdict lands. It sits directly under the
            // thinking lane on purpose: that lane is what the agent is asking,
            // this is the answer — ctx/up/cwd below stay ambient tallies.
            //
            // Same two chips, same wire line, same daemon guards as the
            // herald's own card (herald.qml / herald-center.qml) — this is a
            // SECOND door onto one mechanism, not a second mechanism. The
            // label is the summons summary ("Bash · permission"), which is
            // hook-payload DATA: PlainText, carried, never interpreted.
            Item {
                id: summonsLine
                visible: card.summoned
                width: parent.width
                height: visible ? 20 : 0

                Text {                           // what is being asked
                    anchors.left: parent.left; anchors.leftMargin: 21
                    anchors.right: approveChip.left; anchors.rightMargin: 8
                    anchors.verticalCenter: parent.verticalCenter
                    // the lock is nf-fa-lock, written as an escape — the SAME
                    // glyph the sudo lamp already proves on this stack. (The
                    // key U+26BF was tried first and rendered as tofu in
                    // JetBrainsMono Nerd Font — live-checked, per bar.qml's
                    // unproven-glyph rule.)
                    text: (card.summonsRec && card.summonsRec.summary)
                          ? ("\uf023 " + card.summonsRec.summary)
                          : "\uf023 permission"
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                    font.family: temple.faceMono; font.pixelSize: 9
                    color: temple.livery.paletteUrgent
                }
                Rectangle {                      // approve — the gold verdict
                    id: approveChip
                    anchors.right: denyChip.left; anchors.rightMargin: 8
                    anchors.verticalCenter: parent.verticalCenter
                    width: 66; height: 16
                    radius: 0
                    color: approveMa.containsMouse
                           ? temple.withA(temple.signature, 0.20) : "transparent"
                    border.width: 1
                    border.color: temple.withA(temple.signature,
                                               approveMa.containsMouse ? 0.95 : 0.6)
                    Text {
                        anchors.centerIn: parent
                        text: "approve"
                        font.family: temple.faceMono; font.pixelSize: 9
                        color: temple.signature
                    }
                    MouseArea {
                        id: approveMa
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: temple.verdict(card.s ? card.s.sessionId : "", "approve")
                    }
                }
                Rectangle {                      // deny — the terracotta verdict
                    id: denyChip
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    width: 66; height: 16
                    radius: 0
                    color: denyMa.containsMouse
                           ? temple.withA(temple.livery.paletteUrgent, 0.20) : "transparent"
                    border.width: 1
                    border.color: temple.withA(temple.livery.paletteUrgent,
                                               denyMa.containsMouse ? 0.95 : 0.6)
                    Text {
                        anchors.centerIn: parent
                        text: "deny"
                        font.family: temple.faceMono; font.pixelSize: 9
                        color: temple.livery.paletteUrgent
                    }
                    MouseArea {
                        id: denyMa
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: temple.verdict(card.s ? card.s.sessionId : "", "deny")
                    }
                }
            }

            // ── 5 · PULSE — ctx · up, SHARING one line ────────────────────────
            // ctx (this session's OWN context window, NUMBERS ONLY — no
            // percentage text, no bar glyph; ctxColor severity rides the
            // colour alone; mains only) left, up (elapsed since start, both
            // kinds) right-anchored beside it on mains — the same read-
            // right-to-left corner idiom as row 1's #NN/wsN pair. Subs have
            // no ctx to share with, so up left-anchors alone at the usual
            // margin instead of stranding on the right.
            Item {
                width: parent.width
                height: 13

                Text {                           // ctx label — mains only
                    id: ctxLabel
                    visible: !card.child
                    anchors.left: parent.left; anchors.leftMargin: 21
                    anchors.verticalCenter: parent.verticalCenter
                    text: "ctx"
                    font.family: temple.faceMono; font.pixelSize: 9
                    color: temple.withA(temple.livery.paletteFg, 0.35)
                }
                Text {                           // ctx value — mains only
                    visible: !card.child
                    anchors.left: ctxLabel.right; anchors.leftMargin: 6
                    anchors.right: upLabel.left; anchors.rightMargin: 10
                    anchors.verticalCenter: parent.verticalCenter
                    elide: Text.ElideRight
                    text: card.hasCtx
                          ? (temple.livery.ctxCompact(card.s.contextTokens)
                             + (card.s.contextCeiling
                                ? (" / " + temple.livery.ctxCompact(card.s.contextCeiling))
                                : "") + " tok")
                          : "—"
                    font.family: temple.faceMono; font.pixelSize: 10
                    color: card.hasCtx
                           ? temple.livery.ctxColor(card.ctxPct, temple.signature)
                           : temple.withA(temple.livery.paletteFg, 0.3)
                }
                Text {                           // up label — right corner on
                    id: upLabel                    // mains, left margin on subs
                    anchors.left: card.child ? parent.left : undefined
                    anchors.leftMargin: card.child ? 21 : 0
                    anchors.right: card.child ? undefined : upValue.left
                    anchors.rightMargin: card.child ? 0 : 6
                    anchors.verticalCenter: parent.verticalCenter
                    text: "up"
                    font.family: temple.faceMono; font.pixelSize: 9
                    color: temple.withA(temple.livery.paletteFg, 0.35)
                }
                Text {                           // up value — the clock
                    id: upValue
                    anchors.left: card.child ? upLabel.right : undefined
                    anchors.leftMargin: card.child ? 6 : 0
                    anchors.right: card.child ? undefined : parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    text: temple.livery.elapsedSince(
                              card.s ? card.s.startedAt : "", temple.nowMs)
                    font.family: temple.faceMono
                    font.pixelSize: 10
                    color: card.laurel ? temple.livery.paletteHot
                                       : temple.livery.paletteAccent
                }
            }

            // ── 6s · GROUND — SUB ONLY: the troupe alone, right-anchored in
            // its reserved clipped box.
            Item {
                visible: card.child
                width: parent.width
                height: 15

                Item {                           // reserved, clipped troupe box
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    width: 90; height: 15
                    clip: true
                    Text {
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        text: card.faceText
                        font.family: temple.faceMono; font.pixelSize: 9
                        // state-tinted at 0.85 — the same face-colour grammar
                        // the Terminals troupe wears (withA(accent, 0.85))
                        color: card.laurel ? temple.livery.paletteHot
                                           : temple.withA(temple.lampColor(card.cardLiveState), 0.85)
                    }
                }
            }

            // ── 6m · GROUND — MAIN ONLY, bottom of the card: cwd (left) ── the
            // animated troupe (right), sharing the card's last line. Unmoved
            // by directive — the owner keeps the ground where it is.
            Item {
                visible: !card.child
                width: parent.width
                height: 17

                Item {                           // reserved, clipped troupe box
                    id: faceBox
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    width: 116; height: 17
                    clip: true
                    Text {
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        text: card.faceText
                        font.family: temple.faceMono; font.pixelSize: 10
                        // state-tinted at 0.85 — the same face-colour grammar
                        // the Terminals troupe wears (withA(accent, 0.85))
                        color: card.laurel ? temple.livery.paletteHot
                                           : temple.withA(temple.lampColor(card.cardLiveState), 0.85)
                    }
                }
                Text {                           // cwd — left-anchored, tail
                    anchors.left: parent.left; anchors.leftMargin: 21          // kept via ElideLeft
                    anchors.right: faceBox.left; anchors.rightMargin: 8
                    anchors.verticalCenter: parent.verticalCenter
                    horizontalAlignment: Text.AlignLeft
                    text: card.s && card.s.cwd
                          ? temple.shortPath(card.s.cwd) : "—"
                    elide: Text.ElideLeft
                    font.family: temple.faceMono; font.pixelSize: 10
                    color: card.s && card.s.cwd
                           ? temple.withA(temple.livery.holoBlue, 0.85)
                           : temple.withA(temple.livery.paletteFg, 0.3)
                }
            }
        }

        // hover-preview + trace + click-to-focus (a courier focuses its parent)
        MouseArea {
            id: cardMouse
            acceptedButtons: Qt.LeftButton | Qt.RightButton
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onEntered: {
                if (!temple.shared || !card.s) return
                temple.shared.hoveredWorkspace = card.hasWs ? card.s.workspace : -1
                temple.shared.hoveredSessionId = card.s.sessionId || ""
                temple.shared.tracedSessionId = card.s.sessionId || ""
            }
            onExited: {
                if (!temple.shared || !card.s) return
                if (temple.shared.hoveredSessionId === (card.s.sessionId || "")) {
                    temple.shared.hoveredWorkspace = -1
                    temple.shared.hoveredSessionId = ""
                    temple.shared.tracedSessionId = ""
                }
            }
            onClicked: function(mouse) {
                if (mouse.button === Qt.RightButton) { temple.openSessionMenu(card.s, cardMouse, mouse.x, mouse.y); return }
                if (!temple.bridge || !card.s) return
                var id = (card.child && card.s.parentSessionId)
                         ? card.s.parentSessionId : card.s.sessionId
                if (id) temple.bridge.focusSession(id)
            }
        }
    }
}
