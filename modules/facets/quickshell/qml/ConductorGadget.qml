// ConductorGadget.qml — the agent roster temple (pantheon · Attic gold · 𝄞).
//
// THE PROGRAM — the roster reads as a CONCERT PLAYBILL: a pantheon entablature
// over a hall of collapsible per-project FOLDERS, each folder a numbered
// movement whose cards are main-agent plaques with their subagent plaques hung
// beneath. Self-contained: this file owns its data read (stage/sessions.json +
// projects.json + hooks.json, QS_STAGE honoured), its model build, and its
// card UI. Nothing here is shared with TerminalsGadget — that temple keeps its
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
// ── The 5-slot card template (a stable silhouette, not a grab-bag) ─────────
// A MAIN card always shows the SAME five lines — absent data renders a dim
// placeholder, never a collapsed slot, so every main plaque holds one
// silhouette the eye can scan down:
//   1 IDENTITY  lamp + name/title + kind tag (⟐ sub / ⇄ a2a) + ϟ hook tag ·
//               wsN corner (main agents only)
//   2 PROVENANCE the FULL model id (never abbreviated, ElideMiddle; a dim
//               "—" placeholder when the record lacks one) — the model name
//               is ALWAYS shown, for subagents too
//   3 THINKING  a FIXED box (44px = four 10px lines on mains, 22px = two
//               lines on subs) holding the live voice (▸ activity, else the
//               italic “say …”, else a ϟ phase placeholder); text wraps and
//               the LAST line elides — the fixed height IS the reserved
//               thinking/tool lane, so random tool-usage text popping in
//               never shifts the card
//   4 CONTEXT   the house shade-glyph gauge `[▓▓▓▓░░░░░░] 47% · 95k / 1M tok`
//               — THIS session's OWN window + token usage, never a rollup
//   5 DIRECTORY the cwd breadcrumb on its own right-anchored line, ElideLeft
// The right cluster reads right→left and is TOP-anchored in the thinking box:
// the clipped kaomoji face (116px) ← the elapsed uptime clock (48px, pins the
// box's top-right corner) ← sudo. Same offsets on every card, one column of
// clocks, one column of faces.
//
// ── Reserved space (the anchoring law — text AND animations) ───────────────
// Two animated elements, two reserved side-anchored boxes. The LAMP (the §2
// contract: ♪ 𝄐 𝄼 𝄽 𝄂 · state colours — working pulses via a scale animation,
// awaiting breathes via an opacity animation) lives INSIDE a fixed 16px box at
// the card's top-left; both animations are confined to the box — nothing
// floats, no layout shift. The KAOMOJI TROUPE lives inside the fixed 116px
// clipped box at the thinking slot's top-right; its frames swap text, never
// geometry. The thinking slot itself is a fixed-height reserved lane. Every
// free-length string elides against a fixed partner: name vs tags, model id
// middle-out, thinking against the face box, cwd left-elided. Every text
// anchors to EXACTLY one side (left column left, right column right; flex
// zones use both anchors + elide) — fixed columns, fixed order, right cluster
// reads right→left. Hover previews the card's workspace + takes the trace;
// click → bridge.focusSession (a courier focuses its parent's window). One
// laurel standout (§5): traced card, else first working. All colour from
// `notes`; radius 0 everywhere.
//
// ── The hooks channel (any-agent) ──────────────────────────────────────────
// stage/hooks.json — [{ sessionId, phase, updatedAt }] — is agent-agnostic:
// ANY agent session may be hooked into a phase from outside the roster's own
// state. The hook phase WINS as the card's live state (cardLiveState): it
// drives the lamp glyph/colour, the metronome pulse, the terracotta breath,
// the border, and the kaomoji face — while laurel/firstWorkingId/workingCount
// stay roster-based (stable tallies). Hook surfacing: a ϟ tag on the identity
// row, a "ϟ phase" placeholder in the thinking box when there is no activity
// or say, and a ϟN count in the stylobate tally. Phases outside the §2
// vocabulary fall back to the "·" lamp and the puzzled still — tolerant,
// never a crash. Stale hook ids (absent from the roster) are ignored: they
// produce no rows and never inflate the ϟN tally.
//
// ── Reaping ────────────────────────────────────────────────────────────────
// The liveness reaper (`aoide graph reap`, ~12s timer) sweeps dead sessions
// out of the stage file out-of-band — the widget needs NO tombstone rows: a
// session gone from sessions.json is gone from the roster on the next FileView
// reload; `state: "done"` records stay visible with the done pose (𝄂 lamp,
// ( ´▽｀ ) face) until they leave the file. Stale hooks never resurrect rows;
// rebuild() is crash-free on partial/empty records — every stage field is
// optional (additive-v0 contract).

import QtQuick
import Quickshell
import Quickshell.Io

Item {
    id: temple

    // ── Integration contract ────────────────────────────────────────────────
    required property var notes
    required property var bridge
    property var shared: null

    implicitWidth: 360
    implicitHeight: 520

    // Doric order — Attic gold signature
    readonly property color signature: notes.paletteAccent

    // type voices — the shared three
    readonly property string faceSerif: "Noto Serif"
    readonly property string faceMono:  "JetBrainsMono Nerd Font"
    readonly property string faceMusic: "Noto Music"

    function withA(cstr, a) {
        var c = Qt.darker(cstr, 1.0)
        return Qt.rgba(c.r, c.g, c.b, a)
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
    readonly property string stageDir: {
        var s = Quickshell.env("QS_STAGE")
        return (s && s.length > 0) ? s
             : Quickshell.env("HOME") + "/Aoide/song/stage"
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
    Component.onCompleted: { parseSessions(); parseProjects(); parseHooks() }

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
        case "working":  return notes.paletteAccent
        case "awaiting": return notes.paletteUrgent
        case "stopped":  return notes.holoBlue
        case "idle":     return notes.violet
        case "done":     return notes.wireCyan
        default:         return notes.paletteFg
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
            var p = ps[i] && ps[i].path
            if (!p) continue
            if ((cwd === p || cwd.indexOf(p + "/") === 0) && p.length > bestLen) {
                best = i; bestLen = p.length
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
            if (kindOf(s) !== "shell") mine.push(s)   // agents · subagents · a2a
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

        // Bucket tops (children ride with their parent) into movements.
        var buckets = [], ps = _projects || []
        for (i = 0; i < ps.length; i++) buckets.push({ rows: [], items: [] })
        var adrift = { rows: [], items: [] }
        for (i = 0; i < tops.length; i++) {
            var t = tops[i]
            var pi = projectOf(t.cwd)
            var ch = kids[t.sessionId] || []
            var target = (pi >= 0) ? buckets[pi] : adrift
            target.rows.push({ s: t, kids: ch })
            target.items.push(t)
            for (var j = 0; j < ch.length; j++) target.items.push(ch[j])
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
        color: temple.withA(notes.paletteFg, 0.22)
    }

    Rectangle {
        id: stele
        anchors.fill: parent
        radius: 0
        color: notes.paletteBg
        border.color: notes.paletteFg
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
                    color: temple.withA(notes.paletteFg, 0.05)
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
                        color: notes.paletteFg
                    }
                    Text {
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        text: "[ conductor ]"
                        font.family: temple.faceMono; font.pixelSize: 11
                        color: temple.withA(temple.signature, 0.95)
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
                Text { x: 67; y: 0.6
                       text: "3"
                       font.family: temple.faceSerif; font.italic: true
                       font.pixelSize: 9
                       color: temple.withA(temple.signature, 0.9) }
                Text { x: 111; y: 0.6
                       text: "3"
                       font.family: temple.faceSerif; font.italic: true
                       font.pixelSize: 9
                       color: temple.withA(temple.signature, 0.9) }
                Text { x: 155; y: 0.6
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
                            color: temple.withA(notes.paletteFg, 0.45)
                        }
                        Text {
                            anchors.horizontalCenter: parent.horizontalCenter
                            text: "ᕕ( ᐛ )ᕗ"
                            font.family: temple.faceMono; font.pixelSize: 15
                            color: temple.withA(notes.paletteFg, 0.5)
                        }
                        Text {
                            anchors.horizontalCenter: parent.horizontalCenter
                            text: "no agents on stage"
                            font.family: temple.faceSerif; font.italic: true
                            font.pixelSize: 10
                            color: temple.withA(notes.paletteFg, 0.4)
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
                            g.anchored ? temple.notes.noteColor(g.hueIndex)
                                       : temple.withA(temple.notes.paletteFg, 0.55)
                        readonly property bool headed: g.name !== ""
                        readonly property var roll: temple.notes.ctxRollup(g.items)
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
                                       ? (temple.notes.ctxCompact(movement.roll.sumTok)
                                          + " tok")
                                       : "—"
                                font.family: temple.faceMono; font.pixelSize: 9
                                color: movement.roll.any
                                       ? temple.withA(temple.notes.paletteAccent, 0.9)
                                       : temple.withA(temple.notes.paletteFg, 0.5)
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
                                onClicked:
                                    temple.collapsed[movement.g.name || "adrift"]
                                        = movement.open
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
                                                    color: temple.withA(temple.notes.wireCyan, 0.5)
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

        // slim gold scrollbar in the body gutter
        Item {
            visible: flick.contentHeight > flick.height + 1
            anchors.top: flick.top; anchors.bottom: flick.bottom
            anchors.right: parent.right; anchors.rightMargin: 9
            width: 3
            Rectangle { anchors.fill: parent; color: temple.withA(notes.paletteFg, 0.10) }
            Rectangle {
                width: parent.width
                y: flick.visibleArea.yPosition * parent.height
                height: Math.max(20, flick.visibleArea.heightRatio * parent.height)
                color: temple.withA(temple.signature, 0.75)
            }
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
                                   + temple.notes.ctxCompact(temple.totalTokens)
                                   + " tok · ϟ" + temple.hookedCount)) + " ├"
                font.family: temple.faceMono; font.pixelSize: 11
                color: temple.withA(notes.paletteFg, 0.8)
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
                color: notes.paletteAccent
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
    // A hairline-bordered card on a marble band, standing on its own context
    // meter. The 5-slot template (main) / 3-4-slot compact (sub) — fixed
    // heights, one silhouette per kind. Reserved boxes: lamp 16px left,
    // face 116px right, thinking lane fixed — the anchoring law covers the
    // animated elements too, nothing floats.
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
        readonly property bool cardWorking: cardLiveState === "working"
        readonly property bool cardAwaiting: cardLiveState === "awaiting" || sudoHeld
        readonly property bool cardResting: !cardWorking && !cardAwaiting
        readonly property bool hasCtx: !!(s && s.contextTokens > 0)
        readonly property real ctxPct: hasCtx
            ? temple.notes.ctxPercent(s.contextTokens, s.contextCeiling) : 0
        readonly property bool laurel:
            temple.emphasizedId !== "" && !!s && s.sessionId === temple.emphasizedId
        readonly property string sKind: temple.kindOf(s)
        readonly property bool hasWs:
            !!(s && s.workspace !== undefined && s.workspace !== null)
        readonly property bool hooked: temple.hooked(s ? s.sessionId : "")

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

        // the thinking lane — live voice, else say, else the hook placeholder
        readonly property string liveText: (s && s.activity) ? ("" + s.activity) : ""
        readonly property string sayText: (s && s.say)
            ? ("" + s.say).replace(/\s+/g, " ") : ""
        readonly property bool thinkLive: liveText !== ""
        readonly property string thinkText: thinkLive
            ? ((liveText.indexOf("▸") === 0 ? "" : "▸ ") + liveText
               + (sayText !== "" ? "\n" + sayText : ""))
            : (sayText !== "" ? sayText
               : (card.hooked ? ("ϟ " + card.cardLiveState) : "…"))

        // the plaque ground + hairline (terracotta while awaiting)
        Rectangle {
            anchors.fill: parent
            radius: 0
            color: cardMouse.containsMouse
                   ? temple.withA(temple.signature, 0.10)
                   : (card.laurel ? temple.withA(temple.notes.paletteHot, 0.05)
                                  : temple.withA(temple.notes.paletteFg, 0.035))
            border.width: 1
            border.color: card.cardAwaiting
                          ? temple.withA(temple.notes.paletteUrgent, 0.75)
                          : temple.withA(temple.notes.paletteFg, 0.22)
        }
        // the project pilaster — laurel when this is the one traced plaque
        Rectangle {
            x: 0; width: 2
            height: parent.height
            color: card.laurel ? temple.notes.paletteHot
                               : temple.withA(card.hue, 0.6)
        }

        Column {
            id: inner
            anchors.left: parent.left; anchors.leftMargin: 9
            anchors.right: parent.right; anchors.rightMargin: 6
            anchors.top: parent.top; anchors.topMargin: 5
            spacing: 3

            // ── 1 · IDENTITY — lamp + name + kind tag + ϟ tag · wsN corner ──
            Item {
                width: parent.width
                height: 16

                Text {                           // workspace tag — fixed corner
                    id: wsT
                    visible: !card.child && card.hasWs
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    text: card.hasWs ? ("ws" + card.s.workspace) : ""
                    font.family: temple.faceMono; font.pixelSize: 9
                    color: temple.withA(temple.notes.holoBlue, 0.9)
                }

                Item {                           // the reserved lamp box
                    id: lampBox
                    x: 0
                    anchors.verticalCenter: parent.verticalCenter
                    width: 16; height: 16

                    Text {
                        id: lamp
                        anchors.centerIn: parent
                        text: card.sudoHeld ? "" : temple.lampGlyph(card.cardLiveState)
                        font.family: card.sudoHeld ? temple.faceMono : temple.faceMusic
                        font.pixelSize: 12
                        color: card.laurel
                               ? temple.notes.paletteHot
                               : temple.withA(card.sudoHeld ? temple.notes.paletteUrgent
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
                        // awaiting — a terracotta breath, box-neutral
                        SequentialAnimation on opacity {
                            running: card.cardAwaiting
                            loops: Animation.Infinite
                            alwaysRunToEnd: true
                            NumberAnimation { to: 0.35; duration: 700; easing.type: Easing.InOutSine }
                            NumberAnimation { to: 1.0;  duration: 700; easing.type: Easing.InOutSine }
                        }
                    }
                }

                Text {                           // name/title — elides vs all tags
                    id: nameT
                    anchors.left: lampBox.right; anchors.leftMargin: 5
                    anchors.verticalCenter: parent.verticalCenter
                    text: temple.rowName(card.s)
                    elide: Text.ElideRight
                    width: Math.min(implicitWidth,
                                    parent.width - 21
                                    - (kindTag.visible ? kindTag.implicitWidth + 6 : 0)
                                    - (hookTag.visible ? hookTag.implicitWidth + 6 : 0)
                                    - (wsT.visible ? wsT.implicitWidth + 10 : 0))
                    font.family: temple.faceSerif
                    font.pixelSize: card.child ? 12 : 13
                    font.weight: Font.Medium
                    color: temple.withA(temple.notes.paletteFg, card.child ? 0.85 : 1.0)
                }
                Text {                           // kind tag — WITH the name
                    id: kindTag
                    visible: card.sKind === "subagent" || card.sKind === "a2a"
                    anchors.left: nameT.right; anchors.leftMargin: 6
                    anchors.verticalCenter: parent.verticalCenter
                    text: card.sKind === "subagent" ? "⟐ sub" : "⇄ a2a"
                    font.family: temple.faceMono; font.pixelSize: 9
                    color: temple.notes.violet
                }
                Text {                           // the ϟ hook tag
                    id: hookTag
                    visible: card.hooked
                    anchors.left: kindTag.visible ? kindTag.right : nameT.right
                    anchors.leftMargin: 6
                    anchors.verticalCenter: parent.verticalCenter
                    text: "ϟ"
                    font.family: temple.faceMono; font.pixelSize: 9
                    color: temple.withA(temple.lampColor(card.cardLiveState), 0.95)
                }
            }

            // ── 2 · PROVENANCE — the FULL model id (a fixed slot, always) ────
            // Never abbreviated; middle-elided only when it truly must be. The
            // model NAME is always shown — mains AND subs — a dim "—" holding
            // the slot when the record lacks one, so the silhouette never
            // shifts.
            Text {
                anchors.left: parent.left; anchors.leftMargin: 21
                anchors.right: parent.right
                text: (card.s && card.s.model) ? card.s.model : "—"
                elide: Text.ElideMiddle
                font.family: temple.faceMono; font.pixelSize: 9
                color: temple.withA(temple.notes.paletteFg,
                                    (card.s && card.s.model) ? 0.55 : 0.3)
            }

            // ── 3 · THINKING — a FIXED box (4 lines main / 2 lines sub) ─────
            // The right cluster is TOP-anchored and reads right→left: face
            // (116px clipped) ← elapsed uptime clock (48px, pins the corner)
            // ← sudo. The thinking text wraps inside the fixed height and the
            // LAST line elides — the reservation is the lane itself, so tool
            // text popping in never shifts the card.
            Item {
                width: parent.width
                height: card.child ? 22 : 44

                Item {                           // reserved, clipped troupe box
                    id: faceBox
                    anchors.top: parent.top
                    anchors.right: parent.right
                    width: 116; height: 17
                    clip: true
                    Text {
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        text: card.faceText
                        font.family: temple.faceMono; font.pixelSize: 10
                        color: card.laurel ? temple.notes.paletteHot
                                           : temple.withA(temple.notes.paletteFg, 0.6)
                    }
                }
                Text {                           // elapsed — the uptime clock,
                    id: elapsedT                  // pinned top-right of the slot
                    anchors.top: parent.top
                    anchors.right: faceBox.left; anchors.rightMargin: 8
                    width: 48
                    horizontalAlignment: Text.AlignRight
                    text: temple.notes.elapsedSince(
                              card.s ? card.s.startedAt : "", temple.nowMs)
                    font.family: temple.faceMono; font.pixelSize: 10
                    color: card.laurel ? temple.notes.paletteHot
                                       : temple.notes.paletteAccent
                }
                Text {                           // sudo — beside the clock
                    id: sudoT
                    visible: card.sudoHeld
                    anchors.top: parent.top
                    anchors.right: elapsedT.left; anchors.rightMargin: 6
                    text: "sudo"
                    font.family: temple.faceMono; font.pixelSize: 9
                    color: temple.notes.paletteUrgent
                }
                Text {                           // the thinking — wraps, 4-line
                    id: thinkT                     // cap with last-line elide
                    anchors.left: parent.left; anchors.leftMargin: 21
                    anchors.right: sudoT.visible ? sudoT.left : elapsedT.left
                    anchors.rightMargin: 8
                    anchors.top: parent.top
                    height: parent.height
                    verticalAlignment: Text.AlignTop
                    wrapMode: Text.WordWrap
                    elide: Text.ElideRight
                    lineHeight: 10
                    lineHeightMode: Text.FixedHeight
                    text: card.thinkText
                    font.family: (card.thinkLive || card.hooked)
                                 ? temple.faceMono : temple.faceSerif
                    font.italic: !card.thinkLive && !card.hooked
                    font.pixelSize: 10
                    color: card.thinkLive
                           ? temple.withA(temple.notes.paletteFg, 0.65)
                           : (card.hooked
                              ? temple.withA(temple.notes.paletteFg, 0.55)
                              : (card.sayText !== ""
                                 ? temple.withA(temple.notes.paletteFg, 0.55)
                                 : temple.withA(temple.notes.paletteFg, 0.25)))
                }
            }

            // ── 4 · CONTEXT — the window + token usage, its own line ─────────
            // The house [▓▓▓░░░] gauge (notes.ctxBar), filled to THIS session's
            // OWN window, never a rollup. A main plaque keeps the slot even
            // before its first turn — a dim "—" — so the silhouette never
            // shifts; a sub plaque collapses the slot when it has no tokens.
            Item {
                visible: !card.child || card.hasCtx
                width: parent.width
                height: 15

                Text {
                    anchors.left: parent.left; anchors.leftMargin: 21
                    anchors.right: parent.right
                    horizontalAlignment: Text.AlignLeft
                    text: card.hasCtx
                          ? (temple.notes.ctxBar(card.ctxPct, 10) + " "
                             + Math.round(card.ctxPct) + "% · "
                             + temple.notes.ctxCompact(card.s.contextTokens)
                             + (card.s.contextCeiling
                                ? (" / " + temple.notes.ctxCompact(card.s.contextCeiling))
                                : "") + " tok")
                          : "—"
                    elide: Text.ElideRight
                    font.family: temple.faceMono; font.pixelSize: 10
                    color: card.hasCtx
                           ? temple.notes.ctxColor(card.ctxPct, temple.signature)
                           : temple.withA(temple.notes.paletteFg, 0.3)
                }
            }

            // ── 5 · DIRECTORY — the cwd breadcrumb, its own line ─────────────
            // Right-anchored, ElideLeft so the tail always survives; a dim "—"
            // holds the line on a main card without a cwd. Subs collapse it.
            Item {
                visible: !card.child || !!(card.s && card.s.cwd)
                width: parent.width
                height: 14

                Text {
                    anchors.left: parent.left; anchors.leftMargin: 21
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    horizontalAlignment: Text.AlignRight
                    text: card.s && card.s.cwd
                          ? temple.shortPath(card.s.cwd) : "—"
                    elide: Text.ElideLeft
                    font.family: temple.faceMono; font.pixelSize: 10
                    color: card.s && card.s.cwd
                           ? temple.withA(temple.notes.holoBlue, 0.85)
                           : temple.withA(temple.notes.paletteFg, 0.3)
                }
            }
        }

        // hover-preview + trace + click-to-focus (a courier focuses its parent)
        MouseArea {
            id: cardMouse
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
            onClicked: {
                if (!temple.bridge || !card.s) return
                var id = (card.child && card.s.parentSessionId)
                         ? card.s.parentSessionId : card.s.sessionId
                if (id) temple.bridge.focusSession(id)
            }
        }
    }
}
