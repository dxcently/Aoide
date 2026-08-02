// ConductorGadget.qml — the agent roster temple (Doric · Attic gold · 𝄞).
//
// THE PROGRAM — the roster reads as a CONCERT PLAYBILL OF PLAQUES, not a list
// of lines and not an indented tree. Self-contained: this file owns its data
// read (stage/sessions.json + projects.json, QS_STAGE honoured), its model
// build, and its card UI. Nothing here is shared with TerminalsGadget — that
// temple keeps its own, untouched pattern.
//
// ── The structure ───────────────────────────────────────────────────────────
// PROJECTS are MOVEMENTS: each registered project with live sessions gets a
// numbered movement header — `I · AOIDE ────── ⌈70%⌉ · 2` — in the project's
// own identity hue (notes.noteColor by projects.json index). The header tally
// is the ctxRollup MAX-fill (the movement's hottest window) + top-level count;
// it never replaces a card's own meter. Sessions outside every project fall
// into an unnumbered dim UNANCHORED movement; when NO projects are registered
// at all (a valid, common state) no movement chrome draws — just plaques.
//
// Each top-level agent session is a PLAQUE: a hairline-bordered card on a
// faint marble band. The card carries its project's hue as a 2px left
// pilaster (which-movement stays answerable mid-scroll) and swings its border
// terracotta while awaiting.
//
// SUBAGENTS are smaller plaques HUNG beneath their parent card — indented,
// with a └ hanger in the gutter — each carrying its own name/title AND the
// `⟐ sub` kind tag travelling with the name (never instead of it), its own
// full model id, the same status/voice line (▸ activity when the record has
// one), and its own gauge line when it has tokens. Deeper nesting clamps to
// the nearest top-level ancestor.
//
// ── The 4-slot card template (a stable silhouette, not a grab-bag) ──────────
// A TOP-LEVEL card always shows the SAME four lines — absent data renders a
// dim placeholder, never a collapsed slot, so every main plaque holds one
// silhouette the eye can scan down. Sub cards share the template but may
// collapse slots 2/4 (smaller by design):
//   1 IDENTITY  lamp ♪ + name/title + kind tag (⟐ sub / ⇄ a2a) · wsN corner
//   2 PROVENANCE the FULL model id (never abbreviated, ElideMiddle; dim —
//               placeholder on a main card without one)
//   3 STATUS    the voice (▸ activity, else the italic “say …”) flowing into
//               a FIXED-WIDTH right-aligned elapsed column, which anchors the
//               FIXED-WIDTH kaomoji box — clocks and faces sit at identical
//               offsets on every card, one readable row template
//   4 FOOTING   the house shade-glyph gauge `[▓▓▓▓░░░░░░] 47% · 95k tok`
//               (notes.ctxBar — the Meters-temple convention), THIS session's
//               OWN tokens, never a rollup; a dim empty track `[░░░░░░░░░░] —`
//               before the first turn · cwd right, ElideLeft
//
// ── Reserved space (the overlap discipline) ─────────────────────────────────
// Two animated elements, two reserved boxes. The LAMP (the §2 state contract:
// ♪ 𝄐 𝄼 𝄽 𝄂 · in the shared state-colour spread — working pulses, awaiting
// breathes, sudo swaps a  lock) owns a fixed 16px box at the card's top-left;
// the name anchors PAST it. The KAOMOJI TROUPE (MoodFaces — hashed set per
// session, packages pool for couriers, still poses at rest) owns a fixed-width
// clipped box at the status line's right edge; the status text anchors its
// right edge to the box's left. Both partitions are measured, not floated —
// no content length can push text under either animation.
//
// Every free-length string elides: name against its tag, model id middle-out,
// activity against the face box, say full-width, cwd left-elided against the
// ws/usage tags. Hover previews the card's workspace on the bar + takes the
// trace; click → bridge.focusSession (a courier focuses its parent's window).
// One laurel standout (§5): traced card, else first working. All colour from
// `notes`; radius 0 everywhere; integration contract unchanged
// (width/height/notes/bridge/shared from AoidePanel).

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
        interval: 420
        running: temple.visible && temple.workingCount > 0
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
    Component.onCompleted: { parseSessions(); parseProjects() }

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

    // cwd → "~"-form, last ≤4 segments, "…/"-marked when truncated. The
    // renderer ADDITIONALLY left-elides, so even four long segments keep
    // their tail visible.
    function shortPath(p) {
        if (!p) return ""
        var s = "" + p
        var home = Quickshell.env("HOME")
        if (home && s.indexOf(home) === 0) s = "~" + s.substring(home.length)
        var abs = s.charAt(0) === "/"
        var parts = s.split("/").filter(function (x) { return x.length > 0 })
        if (parts.length > 4)
            return "…/" + parts.slice(parts.length - 4).join("/")
        return (abs ? "/" : "") + parts.join("/")
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

    // ── The movement/plaque model ────────────────────────────────────────────
    // groups: [{ name, ord (movement numeral, 0 = unnumbered), hueIndex,
    //            anchored, rows: [{ s, kids: [session…] }], items: [all] }]
    property var groups: []
    property string firstWorkingId: ""
    property int totalCount: 0
    property int workingCount: 0

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
        var total = 0, work = 0, fw = ""
        for (i = 0; i < gs.length; i++) {
            var rows = gs[i].rows
            for (j = 0; j < rows.length; j++) {
                var r = rows[j]
                total++
                if (r.s.state === "working") {
                    work++
                    if (fw === "") fw = r.s.sessionId || ""
                }
                for (var q = 0; q < r.kids.length; q++) {
                    total++
                    if (r.kids[q].state === "working") work++
                }
            }
        }
        temple.groups = gs
        temple.totalCount = total
        temple.workingCount = work
        temple.firstWorkingId = fw
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

        // ── ENTABLATURE: 𝄞 · CONDUCTOR · [ conductor ] ───────────────────────
        Column {
            id: head
            anchors.left: parent.left; anchors.right: parent.right
            anchors.top: parent.top
            anchors.margins: 13
            spacing: 6

            Item {
                width: parent.width
                height: 34

                Rectangle {                      // deeper marble band
                    anchors.fill: parent; anchors.bottomMargin: 4
                    color: temple.withA(notes.paletteFg, 0.05)
                }
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

            Text {                               // Greek-key meander course
                width: parent.width
                clip: true
                text: "┏┛┗┓".repeat(40)
                font.family: temple.faceMono; font.pixelSize: 10
                color: temple.withA(temple.signature, 0.85)
            }

            Item {                               // ┌─┤ ♪ program ├──────┐
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

                // ── MOVEMENTS ────────────────────────────────────────────────
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

                        width: playbill.width
                        spacing: 4

                        // movement header — I · AOIDE ────── ⌈70%⌉ · 2
                        Item {
                            visible: movement.headed
                            width: parent.width
                            height: visible ? 16 : 0

                            Text {
                                id: gName
                                anchors.left: parent.left
                                anchors.verticalCenter: parent.verticalCenter
                                text: (movement.g.anchored
                                       ? temple.roman(movement.g.ord) + " · " : "")
                                      + movement.g.name.toUpperCase()
                                font.family: temple.faceSerif; font.pixelSize: 11
                                font.weight: Font.Medium; font.letterSpacing: 3
                                elide: Text.ElideRight
                                // long project names shrink against the tally
                                width: Math.min(implicitWidth,
                                                parent.width - gStat.implicitWidth - 20)
                                color: movement.hue
                            }
                            Text {
                                // MAX-fill rollup (the movement's hottest
                                // window — additional, never replacing a
                                // card's own meter) + the plaque count.
                                id: gStat
                                anchors.right: parent.right
                                anchors.verticalCenter: parent.verticalCenter
                                text: (movement.roll.any
                                       ? ("⌈" + Math.round(movement.roll.maxFill) + "%⌉ · ")
                                       : "")
                                      + movement.g.rows.length
                                font.family: temple.faceMono; font.pixelSize: 9
                                color: movement.roll.any
                                       ? temple.notes.ctxColor(movement.roll.maxFill,
                                                               temple.notes.paletteAccent)
                                       : temple.withA(temple.notes.paletteFg, 0.5)
                            }
                            Rectangle {
                                anchors.left: gName.right; anchors.leftMargin: 8
                                anchors.right: gStat.left; anchors.rightMargin: 8
                                anchors.verticalCenter: parent.verticalCenter
                                height: 1
                                color: temple.withA(movement.hue, 0.3)
                            }
                        }

                        // the movement's plaques (+ hung courier plaques)
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
                anchors.verticalCenter: parent.verticalCenter
                text: "└─┤ " + (temple.totalCount === 0
                                ? "tacet"
                                : (temple.totalCount + " voices · "
                                   + temple.workingCount + " working")) + " ├"
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
                anchors.leftMargin: 2; anchors.rightMargin: 6
                anchors.verticalCenter: parent.verticalCenter
                anchors.verticalCenterOffset: 1
                height: 1
                color: temple.withA(temple.signature, 0.55)
            }
        }
    }

    // ═════ ONE PLAQUE ══════════════════════════════════════════════════════════
    // A hairline-bordered card on a marble band, standing on its own context
    // meter. Two reserved animation boxes (lamp top-left, troupe status-right)
    // partition the card's width against the text — nothing floats.
    component SessionCard: Item {
        id: card

        property var s: ({})
        property bool child: false
        property color hue: temple.signature

        width: parent ? parent.width : 0
        implicitHeight: inner.implicitHeight + 11   // 5 top + text + 6 base

        readonly property string sState: (s && s.state) ? ("" + s.state).toLowerCase() : ""
        readonly property bool sudoHeld: !!(s && s.needsSudo)
        readonly property bool cardWorking: sState === "working"
        readonly property bool cardAwaiting: sState === "awaiting" || sudoHeld
        readonly property bool cardResting: !cardWorking && !cardAwaiting
        readonly property bool hasCtx: !!(s && s.contextTokens > 0)
        readonly property real ctxPct: hasCtx
            ? temple.notes.ctxPercent(s.contextTokens, s.contextCeiling) : 0
        readonly property bool laurel:
            temple.emphasizedId !== "" && !!s && s.sessionId === temple.emphasizedId
        readonly property string sKind: temple.kindOf(s)
        readonly property bool hasWs:
            !!(s && s.workspace !== undefined && s.workspace !== null)

        // troupe casting — hashed per session id, couriers from the packages
        // pool, everyone else from the general pool; resting states hold the
        // shared still pose for their state.
        readonly property var facePool:
            sKind === "subagent" ? faces.packages : faces.working
        readonly property int faceSet:
            faces.pickFor((s && s.sessionId) || "", facePool)
        readonly property var faceFrames: faces.workingFrames(faceSet, facePool)
        readonly property string faceText: cardWorking
            ? faceFrames[(temple.faceTick
                          + faces.phaseFor((s && s.sessionId) || "", faceFrames.length))
                         % faceFrames.length]
            : faces.still(sState, true)

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
            spacing: 2

            // ── 1 · IDENTITY — lamp + name/title + kind tag · wsN corner ─────
            Item {
                width: parent.width
                height: 16

                Text {                           // workspace tag — fixed corner
                    id: wsT
                    visible: card.hasWs
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
                        text: card.sudoHeld ? "" : temple.lampGlyph(card.sState)
                        font.family: card.sudoHeld ? temple.faceMono : temple.faceMusic
                        font.pixelSize: 12
                        color: card.laurel
                               ? temple.notes.paletteHot
                               : temple.withA(card.sudoHeld ? temple.notes.paletteUrgent
                                                            : temple.lampColor(card.sState),
                                              card.cardResting ? 0.55 : 1.0)

                        // working — a metronome pulse
                        SequentialAnimation on scale {
                            running: card.cardWorking
                            loops: Animation.Infinite
                            alwaysRunToEnd: true
                            NumberAnimation { to: 1.35; duration: 520; easing.type: Easing.InOutSine }
                            NumberAnimation { to: 1.0;  duration: 520; easing.type: Easing.InOutSine }
                        }
                        // awaiting — a terracotta breath
                        SequentialAnimation on opacity {
                            running: card.cardAwaiting
                            loops: Animation.Infinite
                            alwaysRunToEnd: true
                            NumberAnimation { to: 0.35; duration: 700; easing.type: Easing.InOutSine }
                            NumberAnimation { to: 1.0;  duration: 700; easing.type: Easing.InOutSine }
                        }
                    }
                }

                Text {                           // name/title — elides vs both tags
                    id: nameT
                    anchors.left: lampBox.right; anchors.leftMargin: 5
                    anchors.verticalCenter: parent.verticalCenter
                    text: temple.rowName(card.s)
                    elide: Text.ElideRight
                    width: Math.min(implicitWidth,
                                    parent.width - 21
                                    - (kindTag.visible ? kindTag.implicitWidth + 6 : 0)
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
            }

            // ── 2 · PROVENANCE — the FULL model id (a fixed slot on mains) ───
            // Never abbreviated; middle-elided only when it truly must be. A
            // main card without one holds the slot with a dim placeholder so
            // the 4-line silhouette never shifts; a sub card may collapse it.
            Text {
                visible: !card.child || !!(card.s && card.s.model)
                anchors.left: parent.left; anchors.leftMargin: 21
                anchors.right: parent.right
                text: (card.s && card.s.model) ? card.s.model : "—"
                elide: Text.ElideMiddle
                font.family: temple.faceMono; font.pixelSize: 9
                color: temple.withA(temple.notes.paletteFg,
                                    (card.s && card.s.model) ? 0.55 : 0.3)
            }

            // ── 3 · STATUS — voice · elapsed (fixed column) · the troupe ─────
            // One composed line, not bolted boxes: the left voice (▸ activity
            // when the record carries one — subagents included — else the
            // italic “say …”) flows up to a FIXED-WIDTH right-aligned elapsed
            // column, which itself anchors the FIXED-WIDTH clipped troupe box.
            // Clock and face sit at identical offsets from the card's right
            // edge on EVERY card, so scanning down the roster reads one column
            // of clocks and one column of faces.
            Item {
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
                        color: card.laurel ? temple.notes.paletteHot
                                           : temple.withA(temple.notes.paletteFg, 0.6)
                    }
                }
                Text {                           // elapsed — the fixed clock column
                    id: elapsedT
                    anchors.right: faceBox.left; anchors.rightMargin: 8
                    anchors.verticalCenter: parent.verticalCenter
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
                    anchors.right: elapsedT.left; anchors.rightMargin: 6
                    anchors.verticalCenter: parent.verticalCenter
                    text: "sudo"
                    font.family: temple.faceMono; font.pixelSize: 9
                    color: temple.notes.paletteUrgent
                }
                Text {                           // the voice — live tool, else say
                    readonly property bool live: !!(card.s && card.s.activity)
                    visible: live || !!(card.s && card.s.say)
                    anchors.left: parent.left; anchors.leftMargin: 21
                    anchors.right: sudoT.visible ? sudoT.left : elapsedT.left
                    anchors.rightMargin: 8
                    anchors.verticalCenter: parent.verticalCenter
                    text: live ? ("▸ " + card.s.activity)
                               : ("“" + (((card.s && card.s.say) || "")
                                         .replace(/\s+/g, " ")) + "”")
                    elide: Text.ElideRight
                    font.family: live ? temple.faceMono : temple.faceSerif
                    font.italic: !live
                    font.pixelSize: 10
                    color: temple.withA(temple.notes.paletteFg, live ? 0.65 : 0.55)
                }
            }

            // ── 4 · FOOTING — the shade-glyph gauge · cwd ────────────────────
            // The house [▓▓▓░░░] convention (notes.ctxBar — the same gauge the
            // Meters temple reads), filled to THIS session's OWN window, never
            // a rollup. A main plaque keeps the slot even before its first
            // turn — a dim empty track — so the silhouette never shifts.
            Item {
                visible: !card.child || card.hasCtx || !!(card.s && card.s.cwd)
                width: parent.width
                height: 15

                Text {
                    id: gaugeT
                    anchors.left: parent.left; anchors.leftMargin: 21
                    anchors.verticalCenter: parent.verticalCenter
                    text: temple.notes.ctxBar(card.ctxPct, 10)
                          + (card.hasCtx
                             ? (" " + Math.round(card.ctxPct) + "% · "
                                + temple.notes.ctxCompact(card.s.contextTokens) + " tok")
                             : " —")
                    font.family: temple.faceMono; font.pixelSize: 10
                    color: card.hasCtx
                           ? temple.notes.ctxColor(card.ctxPct, temple.signature)
                           : temple.withA(temple.notes.paletteFg, 0.3)
                }
                Text {                           // cwd — tail always survives
                    visible: !!(card.s && card.s.cwd)
                    anchors.left: gaugeT.right; anchors.leftMargin: 10
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    horizontalAlignment: Text.AlignRight
                    text: temple.shortPath(card.s ? card.s.cwd : "")
                    elide: Text.ElideLeft
                    font.family: temple.faceMono; font.pixelSize: 10
                    color: temple.withA(temple.notes.holoBlue, 0.85)
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
