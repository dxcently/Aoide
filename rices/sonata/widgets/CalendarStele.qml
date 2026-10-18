// CalendarStele.qml — sonata/Greek-marble "calendar" flavor widget: the COMPOSITE
// order carried on a PAPYRUS SCROLL — a self-framed stele that unrolls.
//
// ── Two modes on one sheet ──────────────────────────────────────────────────
//
// A compact month view and an expanded lunisolar year share one sheet. The
// compact sheet is 256 wide; the expanded window is 615 × 784 and its body is
// laid out on the same grid. The toggle morphs between them: both rollers
// lengthen and the two body layers slide under the sheet's clip, so the change
// reads as material feeding through the rolls, not a reflow.
//
// The expanded year is the Attic civil calendar made layout. Its frame is the
// solar year, 3×4 Gregorian month blocks, and its content is lunar. Each block
// marks its computed noumenia day(s) with an aegean box (holoBlue, the
// information role) and inscribes the Attic month that begins there. The labels
// roll over at the real lunations, so Ποσειδεών β′ appears in embolismic years
// and Ἑκατομβαιών lands mid-summer. The footer tally reads the Metonic position
// ("meton N of 19 · 12/13 months"). The compact grid carries the same noumenia
// box on its big cells.
//   Moon arithmetic is mean-lunation (Meeus: JDE 2451550.09766 + 29.530588861k):
//   day-level, within ±1 day of the true phase.
//   A rice widget, not an ephemeris. The solstice is approximated as June 21, and
//   noumeniai are dated in UTC.
//   The toggle is the ἔτος ⌄ / μήν ⌃ chip beside the answer line, or the bottom
//   roller itself, the rod you pull to unroll more. A click on a month block in
//   the expanded year jumps the compact view to that month.
//
// ── Layout ──────────────────────────────────────────────────────────────────
//
// The compact nav row is 232 wide (256 sheet − 2×12). It holds month paging on
// the left and year paging on the right, with 55 px of clear air between them at
// the widest month name. The toggle does not share that row; it is a drawn chip
// in the answer line, 62×20, a hairline box with radius 0 and a clay border that
// fills faintly on hover (notifications.qml's action-button idiom). A tag, such as
// [ token ], marks identity and takes no hit region; a control has a pressable edge.
// The chip's width is fixed, so the ἔτος/μήν swap does not move the footprint.
//
// Expanded cells are 22×16, numerals 11, phase marks 8, the week rail 16 wide with
// 9 px numerals, blocks 177×137, Gregorian month names 13, Attic names 11, the
// memento 10, block gap 12, row gap 8 and block inner spacing 3. Numerals sit
// CENTERED: at this size with an 8 px moon, centring keeps a corner moon nearer
// its own numeral than the next day's.
//
// Sizing discipline: implicitWidth and implicitHeight are constants within a mode.
// Paging never resizes the window, and the open-unfurl animates painted heights
// only. The toggle is the one sanctioned window resize; it steps the window, and
// every visible surface (sheet, rollers, content) animates as paint.
//
// ── Paging and the grids ────────────────────────────────────────────────────
//
// Paging unscrolls. Every paging gesture (‹ ›, « », the three wheels, and the
// month and year home-clicks) routes through page(delta), which turns the data
// swap into material motion on the same body-layer slide the mode morph uses:
// grabToImage snapshots the body's current face into pageGhost (one Image, with
// no second model), the data swaps beneath it, and one pageShift slide moves the
// ghost and the live layer in lockstep through bodyRegion's clip. Forward pulls
// new days up from the bottom roll; backward rolls them down from the top. The
// motion is 220 ms OutCubic and painted only; the window never moves. Re-paging
// mid-slide swaps data under the running motion, so a fast wheel reads as fast
// scrolling. Paging during a morph, or after a failed grab, falls back to an
// instant swap. The expanded block-click keeps its direct jump, because the mode
// morph is that transition's own motion.
//
// The compact grid is Sunday-first, with an ISO 8601 week gutter. The Greek
// weekday names are Sunday-first ordinals (Δευτέρα/Τρίτη/Τετάρτη/Πέμπτη are the
// "2nd/3rd/4th/5th day", Κυριακή being day one), so a Monday-first grid would
// contradict the letters heading it. Each row is numbered by its Thursday's ISO
// week, which is self-consistent because ISO 8601 defines a week's number as its
// Thursday's week. On a Sunday-first row that covers six of seven days, and only
// the leading Sunday belongs to the prior ISO week. The numbers are dim caption
// ink; the row holding today sits a step brighter.
//
// The expanded mini-grids carry the same week rail at mini scale, silenced by
// opacity rather than `visible:` (the Row keeps its slot) on rows with no day of
// the month. The memento mori row lives inside the expanded body layer, so it
// exists only in the year. Its phrase sits in the answer-line italic at the lower
// left, and the counter, "N days · M weeks left" in dim clay, answers it at the
// lower right. The counter reads the real today (root.now, to Dec 31), not the
// browsed year: a memento counts your days, not the scroll's. The closing 𝄂 rule
// stays an empty stave, and the Metonic tally is unchanged.
//
// ── Phase marks ─────────────────────────────────────────────────────────────
//
// PHASE GLYPHS (U+1F311–1F318, EMOJI presentation) are a sanctioned colour
// exception. The moon emoji are absent from all three declared faces (fc-list
// verified), so bare they fall back to Noto Color Emoji, which is fixed-colour and
// deaf to `color:`. The text presentation (+U+FE0E) resolves to DejaVu Sans
// monochrome instead, which recolours in theme.* ink and stays legible at 9 px.
// That monochrome set (aegean, ink-is-shadow polarity) is the documented fallback
// should colour emoji ever leave the rig. The phase marks are the one glyph family
// outside the theme.* roles, and the exception is scoped to that family.
//
// EIGHTHS. monthPhases() divides each lunation at k + q/8, for q 0..7: new,
// waxing crescent, first quarter, waxing gibbous, full, waning gibbous, last
// quarter, waning crescent. It uses the same Meeus mean lattice, so q0 lands
// exactly where the Attic months open. The honesty is the noumeniai's: the
// intermediate eighths are midpoints of mean anomaly, not almanac events, and they
// claim day-level accuracy only.
//
// PLACEMENT. A mark lands about every 3.7 days (roughly eight a month). In the
// compact grid (26×17 cells) each phase day carries its emoji in the cell's
// top-right corner, and the numeral keeps its role ink, since a tinted numeral
// would drown the Sunday rubric. The noumenia is the new moon, so it gets no
// separate box; its mark shows the phase. In the expanded mini-grids (22×16) the
// emoji replaces the numeral on phase days, because corner marks do not fit at
// that size, and the grid position still dates it. Today's laurel outranks a phase
// mark in both modes.
//
// ── Composition ─────────────────────────────────────────────────────────────
//
// ORDER COMPOSITE: the volute order (voluta = "rolled", the scroll in stone).
// SIGNATURE clay `theme.base09` is the RUBRIC, the red-ochre ink of the fasti.
// base09 is also worn by AudioColonnade and UsageGadget, and that collision is
// still open for adjudication. CROWN 𝄴, 24 px. KALENDAE at 15 px with loose
// tracking, answered by a small italic ἡμερολόγιον (the AoidePanel bilingual
// inscription idiom, AOIDE ⁄ ᾠδή). FRIEZE: a Vitruvian wave-scroll. RUBRIC: Sunday Κ
// and the Sunday numerals in clay. LAUREL: today's cell only. Π, the order mark,
// stays dormant, like Α Β Γ Δ Θ Ω before it.
//
// The rollers carry an UMBILICUS, the axle rod and end knob (cornua) that a real
// scroll is wound onto, protruding past the spiral ends. The chrome turns with the
// material: the guillemets are rotated, not replaced. Earlier days are wound into
// the top roller and wheel-up pages backward, so the prev arrow points up and the
// next arrow points down. The ‹ › glyphs and their rotated copies ride fixed wrapper
// Items, because a rotated Text's layout box does not rotate with it. The ἔτος ⌄
// and μήν ⌃ wedges (U+2303/2304, native in JetBrainsMono NF) point the way the
// material will go. The bottom roller stays the wordless handle. The wheels are
// invisible, the block-click's motion is the mode morph, the tally's "wound ±"
// is temporal rather than spatial, and the rollers and frieze are physics and
// ornament, not controls.
//
// ── Surface and host ────────────────────────────────────────────────────────
//
// A self-framed stele, hosted bare through StelePopout, with no GadgetFrame. Under
// a GadgetFrame the surface had two captions and two closures, and the glass lost
// the numerals over dark windows. The injected-prop contract is `theme` and
// `bridge` (bridge unused). Qt's Window.visible attached never flips under
// quickshell's proxy windows, so QsWindow.window.visible is the real open and
// close edge. The popout sits on its own layer surface, in the "aoide-calendar"
// layer namespace, which escapes aoide-bar's blur_popups rule. The host's left pin
// (StelePopout anchorEdges and anchorGravity, set by AoideBar) keeps the popup in
// place through the two window steps.
//
// The sheet is fully opaque papyrus: a flat paletteBg fill, matching every other
// stele in the family. The day grid's numerals must not compete with whatever sits
// behind the popup. The cast shadow is drawn as the visible L only (the right and
// bottom strips), so it never tints the sheet's transparent surroundings.
//
// ── The morph and the window ────────────────────────────────────────────────
//
// BOTH ROLLERS ANIMATE on the toggle, and the content moves with them. One
// `modeFrac` drives the whole morph through the same pipeline as the open-unfurl:
// the sheet's width and target height interpolate, the bottom roller travels, and
// the two body layers slide and fade under the sheet's clip. Compact rises into
// the top roll while the year unrolls beneath it, and the motion reverses on
// re-roll. The open-unfurl pins the top under the bar and unrolls the bottom, a
// hung rotulus.
//
// The window steps and never animates per frame. The morph runs on the shared
// MorphState (a same-directory helper, whose header holds the capture). The window
// goes OUT first on expand and IN last on collapse, so the surface is never smaller
// than the paint, and the sheet's border and cast shadow are never clipped mid-morph.
// No timed resize guard is needed: a layer surface is anchored, not re-mapped, so a
// resize does not replay a popup animation.

import QtQuick
import Quickshell   // QsWindow attached — the open/close edge (see header)

Item {
    id: root

    required property var theme
    required property var bridge

    readonly property string faceSerif: "Noto Serif"
    readonly property string faceMono:  "JetBrainsMono Nerd Font"
    readonly property string faceMusic: "Noto Music"
    readonly property color clay: theme.base09
    readonly property color ink: theme.paletteFg
    readonly property color aegean: theme.holoBlue   // the moon's information hue

    function withA(cstr, a) {
        var c = Qt.color(cstr)
        return Qt.rgba(c.r, c.g, c.b, a)
    }

    // ── Mode: compact month ↔ expanded lunisolar year ────────────────────
    // The state, the 0→1 clock and the ORDER around the window step all live
    // in the shared MorphState (same-dir helper; see its header for the capture). What stays here is the one
    // thing the primitive deliberately does not own: HOW this surface changes
    // size. The scroll steps its own winW/winH — window out first on expand,
    // window in last on collapse — so the painted sheet always has a surface
    // at least as large as itself, and the rollers/content lerp inside it.
    MorphState {
        id: mode
        onStepOut: { root.winW = root.expandedWinW; root.winH = root.expandedWinH }
        onStepIn:  { root.winW = root.compactWinW;  root.winH = root.compactWinH  }
    }

    // ── Fixed geometry — constants per mode, interpolated by modeFrac.
    // Window size changes ONLY on the toggle (the sanctioned resize); paging
    // and the open-unfurl never move it ──────────────────────────────────
    readonly property int protrusion: 18           // roller overhang past the sheet
    readonly property int compactSheetW: 256
    readonly property int expandedSheetW: 579   // 3×(16 rail + 160 grid + 1) + 2×12 + 24
    readonly property int compactBodyH: 130        // caps 14 + 4 + grid 112
    readonly property int expandedBodyH: 596       // 4×137 blocks + 4×8 gaps + memento 16
    readonly property int chromeH: 156             // 24 margins + crown 26 + answer 20
                                                   // + frieze 10 + nav 20 + tally 18
                                                   // + close 14 + 6×4 spacing
    readonly property int sheetW: mode.lerpInt(compactSheetW, expandedSheetW)
    readonly property int bodyH: mode.lerpInt(compactBodyH, expandedBodyH)
    readonly property int modeSheetH: chromeH + bodyH
    readonly property int rollerH: 16
    readonly property int sheetY: 15

    // The WINDOW size is stepped by the sequenced toggle above (never
    // animated per frame — see the remap note there); the PAINTED sheet and
    // rollers are what lerp. The host's left pin (StelePopout anchorEdges/
    // anchorGravity override, set by AoideBar) keeps the popup's position
    // fixed through the two window steps, so they read as nothing at all —
    // a centered popup would visibly re-center on each (live-verified).
    readonly property int compactWinW: compactSheetW + 2 * protrusion    // 292
    readonly property int compactWinH: sheetY + chromeH + compactBodyH + rollerH + 1  // 318
    readonly property int expandedWinW: expandedSheetW + 2 * protrusion  // 615
    readonly property int expandedWinH: sheetY + chromeH + expandedBodyH + rollerH + 1  // 784
    property int winW: compactSheetW + 2 * protrusion
    property int winH: sheetY + chromeH + compactBodyH + rollerH + 1
    implicitWidth: winW
    implicitHeight: winH

    // ── Unfurl — the popout opens (painted heights only; no window resize).
    // Qt's Window.visible attached never flips under quickshell's proxy
    // windows (verified live); QsWindow.window is quickshell's own door to
    // the real PopupWindow/PanelWindow, whose `visible` tracks open/close ──
    property real unfurl: 1
    readonly property int rolledH: 26
    readonly property int sheetVisH: Math.round(rolledH + (modeSheetH - rolledH) * unfurl)
    NumberAnimation {
        id: unfurlAnim
        target: root; property: "unfurl"
        from: 0; to: 1; duration: 260; easing.type: Easing.OutCubic
    }
    readonly property var qsWin: QsWindow.window
    readonly property bool winShown: !!(qsWin && qsWin.visible)
    onWinShownChanged: if (winShown) unfurlAnim.restart()
    Component.onCompleted: if (winShown) unfurlAnim.restart()

    property date now: new Date()
    Timer {
        interval: 60000
        running: true
        repeat: true
        onTriggered: root.now = new Date()
    }

    // ── Paging state — months are the unit; a year page is ±12 ───────────
    // Paging unscrolls: page() grabs the body's current
    // face into pageGhost, swaps the data, then one pageShift slide moves
    // ghost and live layer together through bodyRegion's clip — forward
    // pulls new days up from the bottom roll, backward rolls them down
    // from the top. Re-paging mid-slide swaps under the running motion
    // (wheel stays responsive); a failed grab degrades to an instant swap.
    property int monthOffset: 0
    property real pageShift: 0
    property int pageDir: 1
    property var pageGrab: null          // holds the grab: its url dies with it
    function page(delta) {
        if (delta === 0) return
        if (pageAnim.running || mode.busy) {
            root.monthOffset += delta
            return
        }
        var dir = delta > 0 ? 1 : -1
        var ok = bodyRegion.grabToImage(function(result) {
            root.pageGrab = result
            pageGhost.source = result.url
            root.pageDir = dir
            root.monthOffset += delta
            root.pageShift = dir * root.bodyH
            pageAnim.restart()
        })
        if (!ok) root.monthOffset += delta
    }
    NumberAnimation {
        id: pageAnim
        target: root; property: "pageShift"
        to: 0; duration: 220; easing.type: Easing.OutCubic
        onStopped: {
            root.pageShift = 0
            pageGhost.source = ""
            root.pageGrab = null
        }
    }
    readonly property date viewedDate:
        new Date(root.now.getFullYear(), root.now.getMonth() + root.monthOffset, 1)
    readonly property int viewYear: viewedDate.getFullYear()
    readonly property int viewMonth: viewedDate.getMonth()
    readonly property int today: now.getDate()

    readonly property var monthNames: ["January", "February", "March", "April",
        "May", "June", "July", "August", "September", "October", "November", "December"]
    readonly property var dayLetters: ["Κ", "Δ", "Τ", "Τ", "Π", "Π", "Σ"]  // Greek-initial weekday caps

    readonly property int daysInMonth: new Date(viewYear, viewMonth + 1, 0).getDate()
    readonly property int firstWeekday: new Date(viewYear, viewMonth, 1).getDay()
    readonly property int daysInPrevMonth: new Date(viewYear, viewMonth, 0).getDate()
    readonly property int todayRow:
        Math.floor((firstWeekday + today - 1) / 7)   // meaningful only at monthOffset 0

    // The year's remainder, counted from the REAL today (a memento reads
    // your days, not the scroll's): Dec 31 minus now, whole days.
    readonly property int daysLeft:
        Math.round((Date.UTC(now.getFullYear(), 11, 31)
                    - Date.UTC(now.getFullYear(), now.getMonth(), now.getDate()))
                   / 86400000)
    readonly property int weeksLeft: Math.floor(daysLeft / 7)

    // ── ISO 8601 week of a date — a week is numbered by its Thursday ─────
    function isoWeek(y, m, day) {
        var d = new Date(Date.UTC(y, m, day))
        var dn = (d.getUTCDay() + 6) % 7             // Mon=0
        d.setUTCDate(d.getUTCDate() - dn + 3)        // this week's Thursday
        var jan4 = new Date(Date.UTC(d.getUTCFullYear(), 0, 4))
        var jn = (jan4.getUTCDay() + 6) % 7
        var week1Thu = new Date(Date.UTC(d.getUTCFullYear(), 0, 4 - jn + 3))
        return 1 + Math.round((d.getTime() - week1Thu.getTime()) / 604800000)
    }

    // ── Mean-lunation moon + the Attic lunisolar year (see header note) ──
    readonly property real synodic: 29.530588861
    readonly property real nmEpoch: 2451550.09766    // JDE, mean NM of 2000 Jan 6
    readonly property var atticNames: ["Ἑκατομβαιών", "Μεταγειτνιών", "Βοηδρομιών",
        "Πυανεψιών", "Μαιμακτηριών", "Ποσειδεών", "Γαμηλιών", "Ἀνθεστηριών",
        "Ἐλαφηβολιών", "Μουνιχιών", "Θαργηλιών", "Σκιροφοριών"]

    function jdOf(d) { return d.getTime() / 86400000 + 2440587.5 }

    // The Attic year opening after year y's summer solstice: its lunation
    // start-JDs and month names — 13 entries (Ποσειδεών β′ intercalated after
    // Poseideon) when 13 new moons fall before the next solstice.
    function atticYear(y) {
        var sol = jdOf(new Date(Date.UTC(y, 5, 21)))
        var solNext = jdOf(new Date(Date.UTC(y + 1, 5, 21)))
        var k = Math.ceil((sol - root.nmEpoch) / root.synodic)
        var starts = []
        for (var j = k; root.nmEpoch + root.synodic * j < solNext; j++)
            starts.push(root.nmEpoch + root.synodic * j)
        var names = []
        for (var i = 0; i < starts.length; i++) {
            if (starts.length === 13) {
                if (i < 6) names.push(root.atticNames[i])
                else if (i === 6) names.push("Ποσειδεών β′")
                else names.push(root.atticNames[i - 1])
            } else {
                names.push(root.atticNames[i])
            }
        }
        return { starts: starts, names: names }
    }
    readonly property var atticPrev: atticYear(viewYear - 1)   // covers Jan–Jun
    readonly property var atticCur: atticYear(viewYear)        // covers Jul–Dec
    readonly property int goldenNumber: ((viewYear % 19) + 19) % 19 + 1

    // Noumeniai falling inside Gregorian (y, m): [{day, name}]
    function monthMoons(y, m) {
        var out = []
        var years = [root.atticPrev, root.atticCur]
        for (var a = 0; a < 2; a++) {
            var ay = years[a]
            for (var i = 0; i < ay.starts.length; i++) {
                var d = new Date((ay.starts[i] - 2440587.5) * 86400000)
                if (d.getUTCFullYear() === y && d.getUTCMonth() === m)
                    out.push({ day: d.getUTCDate(), name: ay.names[i] })
            }
        }
        return out
    }
    // Phase days inside Gregorian (y, m): [{day, q}], q 0..7 = the eight
    // phases at k + q/8 synodic (new, waxing crescent, first quarter,
    // waxing gibbous, full, waning gibbous, last quarter, waning crescent)
    // on the same mean lattice as the noumeniai, so q0 lands exactly where
    // the Attic months open. Dated in UTC like everything lunar here.
    function monthPhases(y, m) {
        var first = jdOf(new Date(Date.UTC(y, m, 1)))
        var next = jdOf(new Date(Date.UTC(y, m + 1, 1)))
        var out = []
        for (var k = Math.floor((first - root.nmEpoch) / root.synodic) - 1;
             root.nmEpoch + root.synodic * k < next; k++)
            for (var q = 0; q < 8; q++) {
                var d = new Date((root.nmEpoch + root.synodic * (k + q / 8)
                                  - 2440587.5) * 86400000)
                if (d.getUTCFullYear() === y && d.getUTCMonth() === m)
                    out.push({ day: d.getUTCDate(), q: q })
            }
        return out
    }
    // the real color moon emoji — the sanctioned color exception (header)
    readonly property var phaseGlyphs:
        ["\u{1F311}", "\u{1F312}", "\u{1F313}", "\u{1F314}",
         "\u{1F315}", "\u{1F316}", "\u{1F317}", "\u{1F318}"]
    readonly property var viewPhases: monthPhases(viewYear, viewMonth)
    function phaseOf(dayNum) {
        for (var i = 0; i < root.viewPhases.length; i++)
            if (root.viewPhases[i].day === dayNum) return root.viewPhases[i].q
        return -1
    }

    function kaomojiFor() {
        if (root.monthOffset === 0) return "( ´ ▽ ` )"
        if (root.monthOffset < 0) return "(￢_￢ )"
        return "(☆ ≧▽≦)"
    }

    // Tally: compact — day count at home, distance wound while browsing;
    // expanded — the Metonic position of the Attic year opening in viewYear.
    function tallyText() {
        if (mode.expanded)
            return "meton " + root.goldenNumber + " of 19 · "
                 + root.atticCur.starts.length + " months"
        if (root.monthOffset === 0) return "day " + root.today + " of " + root.daysInMonth
        var a = Math.abs(root.monthOffset)
        var y = Math.floor(a / 12), m = a % 12
        var d = (y > 0 ? y + "y" : "") + (y > 0 && m > 0 ? " " : "") + (m > 0 ? m + "m" : "")
        return "wound " + (root.monthOffset < 0 ? "-" : "+") + d
    }

    // ── Cast shadow — shared stele idiom; rides the reveal edge. Drawn as
    // the visible L only (right + bottom strips). A full shifted rect would
    // underlap the whole sheet, and the window's clear area would show it
    // through the transparent surroundings, tinting the desktop ───────────────
    Rectangle {   // right strip
        x: sheet.x + sheet.width
        y: sheet.y + 5
        width: 4
        height: sheet.height
        radius: 0
        color: root.withA(root.ink, 0.22)
    }
    Rectangle {   // bottom strip (right strip owns the 4×5 corner overlap)
        x: sheet.x + 4
        y: sheet.y + sheet.height
        width: sheet.width - 4
        height: 5
        radius: 0
        color: root.withA(root.ink, 0.22)
    }

    // ── The sheet — fully opaque papyrus: a flat paletteBg fill, matching every other stele in this family (see the header, "Surface and host") ──────────────
    Rectangle {
        id: sheet
        x: root.protrusion
        y: root.sheetY
        width: root.sheetW
        height: root.sheetVisH
        radius: 0
        color: root.theme.paletteBg
        border.color: root.ink
        border.width: 2
        clip: true

        // inset keyline — the rubric hue
        Rectangle {
            anchors.fill: parent; anchors.margins: 4
            radius: 0; color: "transparent"
            border.color: root.withA(root.clay, 0.8); border.width: 1
        }

        Column {
            id: mainColumn
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.margins: 12
            spacing: 4

            // ── Crown row: 𝄴 + KALENDAE carved serif + [ fasti ] tag ─────
            Item {
                width: parent.width
                height: 26
                Text {
                    id: crown
                    anchors.left: parent.left
                    anchors.verticalCenter: parent.verticalCenter
                    text: "𝄴"
                    font.family: root.faceMusic
                    font.pixelSize: 24
                    color: root.clay
                }
                Text {
                    anchors.left: crown.right; anchors.leftMargin: 9
                    anchors.verticalCenter: parent.verticalCenter
                    text: "KALENDAE"
                    font.family: root.faceSerif
                    font.pixelSize: 15
                    font.weight: Font.DemiBold
                    font.letterSpacing: 4
                    color: root.ink
                }
                Text {
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    text: "[ fasti ]"
                    font.family: root.faceMono
                    font.pixelSize: 10
                    color: root.withA(root.clay, 0.9)
                }
            }

            // ── The answer line — AoidePanel's bilingual-inscription idiom
            // (AOIDE ⁄ ᾠδή): the carved Latin name answered in small Greek —
            // and the MODE LINE: the inscription names
            // the sheet, the toggle names which sheet you are looking at ──
            Item {
                width: parent.width
                height: 20
                Text {
                    id: answerGreek
                    anchors.left: parent.left; anchors.leftMargin: 33
                    anchors.verticalCenter: parent.verticalCenter
                    text: "ἡμερολόγιον"
                    font.family: root.faceSerif
                    font.pixelSize: 10
                    font.italic: true
                    color: root.withA(root.ink, 0.5)
                }

                // The mode toggle — ἔτος ⌄ unrolls the year downward, μήν ⌃
                // rolls back up. The bottom roller is the other handle; the
                // wedges point where the material will go. The DRAWN chip
                // (notifications.qml's action-button idiom: hairline border,
                // radius 0, a faint signature fill on hover) is what tells it
                // apart from [ fasti ] one line above — a control has a
                // pressable edge, a tag is only ever letters.
                Rectangle {
                    id: modeToggle
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    width: 62
                    height: 20
                    radius: 0
                    color: togMa.containsMouse ? root.withA(root.clay, 0.16) : "transparent"
                    border.width: 1
                    border.color: root.withA(root.clay, togMa.containsMouse ? 0.9 : 0.55)
                    Text {
                        anchors.centerIn: parent
                        text: mode.expanded ? "μήν ⌃" : "ἔτος ⌄"
                        font.family: root.faceMono
                        font.pixelSize: 10
                        color: root.clay
                        opacity: togMa.containsMouse ? 1.0 : 0.85
                    }
                    MouseArea {
                        id: togMa
                        anchors.fill: parent; anchors.margins: -5
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: mode.toggle()
                    }
                }
            }

            // ── Frieze — Vitruvian wave-scroll (the roll's curl, running) ─
            Canvas {
                id: frieze
                width: parent.width
                height: 10
                onWidthChanged: requestPaint()
                onPaint: {
                    var ctx = getContext("2d")
                    ctx.reset()
                    ctx.clearRect(0, 0, width, height)
                    ctx.strokeStyle = root.clay
                    ctx.lineWidth = 1.3
                    var unit = 18
                    var midY = height / 2
                    var maxR = height * 0.42
                    var n = Math.ceil(width / unit) + 1
                    for (var i = 0; i < n; i++) {
                        var cx = i * unit + unit * 0.35
                        ctx.beginPath()
                        var steps = 24
                        for (var s = 0; s <= steps; s++) {
                            var t = s / steps
                            var ang = t * Math.PI * 3          // 1.5 turns
                            var r = maxR * (1 - t)
                            var x = cx + r * Math.cos(ang)
                            var y = midY + r * Math.sin(ang) * 0.6
                            if (s === 0) ctx.moveTo(x, y); else ctx.lineTo(x, y)
                        }
                        ctx.stroke()
                        ctx.beginPath()
                        ctx.moveTo(cx + maxR, midY)
                        ctx.lineTo((i + 1) * unit + unit * 0.35 - maxR, midY)
                        ctx.stroke()
                    }
                }
            }

            // ── Nav row: ‹ month › (compact only) · [ ἔτος / μήν ] toggle ·
            // « year ». Single chevrons page the MONTH, double the YEAR ────
            Item {
                width: parent.width
                height: 20

                // The guillemets turn with the material:
                // prev points UP the roll (earlier days wound in the top
                // roller), next points DOWN. Rotated Texts ride wrapper
                // Items — a rotated Text's layout box does not rotate.
                Row {
                    id: monthCluster
                    anchors.left: parent.left
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 5
                    opacity: 1 - mode.frac
                    visible: mode.frac < 0.999
                    enabled: !mode.expanded
                    Item {
                        width: 12; height: 18
                        anchors.verticalCenter: parent.verticalCenter
                        Text {
                            anchors.centerIn: parent
                            rotation: 90
                            text: "‹"
                            color: root.clay
                            opacity: mPrevMa.containsMouse ? 1.0 : 0.7
                            font.family: root.faceSerif
                            font.pixelSize: 16
                            font.bold: true
                        }
                        MouseArea {
                            id: mPrevMa
                            anchors.fill: parent; anchors.margins: -3
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: root.page(-1)
                        }
                    }
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        text: root.monthNames[root.viewMonth]
                        color: root.ink
                        font.family: root.faceSerif
                        font.pixelSize: 14
                        font.bold: true
                        MouseArea {
                            anchors.fill: parent; anchors.margins: -3
                            cursorShape: Qt.PointingHandCursor
                            onClicked: root.page(-root.monthOffset)
                        }
                    }
                    Item {
                        width: 12; height: 18
                        anchors.verticalCenter: parent.verticalCenter
                        Text {
                            anchors.centerIn: parent
                            rotation: 90
                            text: "›"
                            color: root.clay
                            opacity: mNextMa.containsMouse ? 1.0 : 0.7
                            font.family: root.faceSerif
                            font.pixelSize: 16
                            font.bold: true
                        }
                        MouseArea {
                            id: mNextMa
                            anchors.fill: parent; anchors.margins: -3
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: root.page(1)
                        }
                    }
                }

                Row {
                    id: yearCluster
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 4
                    Item {
                        width: 13; height: 18
                        anchors.verticalCenter: parent.verticalCenter
                        Text {
                            anchors.centerIn: parent
                            rotation: 90
                            text: "«"
                            color: root.clay
                            opacity: yPrevMa.containsMouse ? 1.0 : 0.7
                            font.family: root.faceSerif
                            font.pixelSize: 15
                            font.bold: true
                        }
                        MouseArea {
                            id: yPrevMa
                            anchors.fill: parent; anchors.margins: -3
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: root.page(-12)
                        }
                    }
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        text: "" + root.viewYear
                        color: root.theme.paletteAccent
                        font.family: root.faceSerif
                        font.pixelSize: 14
                        font.bold: true
                        MouseArea {
                            anchors.fill: parent; anchors.margins: -3
                            cursorShape: Qt.PointingHandCursor
                            onClicked: root.page(-root.monthOffset)
                        }
                    }
                    Item {
                        width: 13; height: 18
                        anchors.verticalCenter: parent.verticalCenter
                        Text {
                            anchors.centerIn: parent
                            rotation: 90
                            text: "»"
                            color: root.clay
                            opacity: yNextMa.containsMouse ? 1.0 : 0.7
                            font.family: root.faceSerif
                            font.pixelSize: 15
                            font.bold: true
                        }
                        MouseArea {
                            id: yNextMa
                            anchors.fill: parent; anchors.margins: -3
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: root.page(12)
                        }
                    }
                }

                MouseArea {
                    anchors.left: yearCluster.left; anchors.right: yearCluster.right
                    anchors.top: yearCluster.top; anchors.bottom: yearCluster.bottom
                    anchors.margins: -4
                    acceptedButtons: Qt.NoButton
                    onWheel: {
                        root.page((wheel.angleDelta.y > 0) ? -12 : 12)
                        wheel.accepted = true
                    }
                }
            }

            // ── The BODY REGION — its height IS the mode morph; the two
            // layers slide under the sheet's clip so the toggle reads as
            // material feeding through the rolls, not a reflow ────────────
            Item {
                id: bodyRegion
                width: parent.width
                height: root.bodyH
                clip: true

                // the outgoing sheet face — a one-frame grab that slides
                // out in lockstep with the live layer sliding in, so the
                // page turn reads as continuous material
                Image {
                    id: pageGhost
                    x: 0
                    y: Math.round(root.pageShift) - root.pageDir * root.bodyH
                    width: bodyRegion.width
                    height: root.bodyH
                    visible: source != "" && root.pageShift !== 0
                }

                // ══ COMPACT: week-number gutter + weekday caps + month grid
                Column {
                    id: compactBody
                    anchors.horizontalCenter: parent.horizontalCenter
                    y: Math.round(-24 * mode.frac + root.pageShift)
                    spacing: 4
                    opacity: Math.max(0, 1 - mode.frac * 2.2)
                    visible: opacity > 0.01

                    // caps row — leading spacer over the week-number gutter;
                    // dim ink caps, Sunday's Κ in rubric clay
                    Row {
                        height: 14
                        spacing: 2
                        Item { width: 20; height: 1 }
                        Repeater {
                            model: root.dayLetters
                            delegate: Text {
                                required property var modelData
                                required property int index
                                width: 26
                                horizontalAlignment: Text.AlignHCenter
                                text: modelData
                                color: index === 0 ? root.clay : root.withA(root.ink, 0.6)
                                font.family: root.faceSerif
                                font.pixelSize: 12
                                font.bold: true
                            }
                        }
                    }

                    // the fixed 42-cell grid, ISO week numbers in the gutter
                    Item {
                        id: gridWrap
                        width: grid.width
                        height: grid.height

                        Column {
                            id: grid
                            spacing: 2

                            Repeater {
                                model: 6
                                delegate: Row {
                                    id: weekRow
                                    required property int index
                                    readonly property int weekIdx: index
                                    readonly property bool holdsToday:
                                        root.monthOffset === 0 && index === root.todayRow
                                    spacing: 2

                                    // ISO week number — informational gutter
                                    // caption, numbered by the row's Thursday
                                    Text {
                                        width: 20
                                        height: 17
                                        horizontalAlignment: Text.AlignRight
                                        verticalAlignment: Text.AlignVCenter
                                        rightPadding: 3
                                        text: root.isoWeek(root.viewYear, root.viewMonth,
                                            weekRow.weekIdx * 7 + 4 - root.firstWeekday + 1)
                                        color: root.withA(root.ink, weekRow.holdsToday ? 0.65 : 0.32)
                                        font.family: root.faceMono
                                        font.pixelSize: 8
                                    }

                                    Repeater {
                                        model: 7
                                        delegate: Rectangle {
                                            required property int index
                                            readonly property int cellIdx: weekRow.weekIdx * 7 + index
                                            readonly property int dayOffset: cellIdx - root.firstWeekday + 1
                                            readonly property bool isPrev: dayOffset < 1
                                            readonly property bool isNext: dayOffset > root.daysInMonth
                                            readonly property bool isCurrent: !isPrev && !isNext
                                            readonly property bool isSunday: index === 0
                                            readonly property int dayNum:
                                                isPrev ? (root.daysInPrevMonth + dayOffset)
                                                       : (isNext ? (dayOffset - root.daysInMonth) : dayOffset)
                                            readonly property bool isToday:
                                                root.monthOffset === 0 && isCurrent && dayNum === root.today
                                            readonly property int phaseQ:
                                                (isCurrent && !isToday) ? root.phaseOf(dayNum) : -1

                                            width: 26
                                            height: 17
                                            radius: 0
                                            color: isToday ? root.theme.paletteHot : "transparent"

                                            Text {
                                                anchors.centerIn: parent
                                                text: parent.dayNum
                                                color: parent.isToday ? root.theme.paletteBg
                                                       : (parent.isCurrent
                                                           ? (parent.isSunday ? root.withA(root.clay, 0.95) : root.ink)
                                                           : root.withA(root.ink, 0.3))
                                                font.family: root.faceSerif
                                                font.pixelSize: 12
                                                font.bold: parent.isToday
                                            }
                                            // the phase itself, corner-inscribed —
                                            // color emoji by sanction; aegean is
                                            // the monochrome-fallback ink (header)
                                            Text {
                                                visible: parent.phaseQ >= 0
                                                anchors.top: parent.top
                                                anchors.right: parent.right
                                                anchors.topMargin: -2
                                                text: parent.phaseQ >= 0
                                                      ? root.phaseGlyphs[parent.phaseQ] : ""
                                                font.family: root.faceSerif
                                                font.pixelSize: 9
                                                color: root.aegean
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        MouseArea {
                            anchors.fill: parent
                            anchors.margins: -4
                            acceptedButtons: Qt.NoButton
                            // wheel pages months; Shift+wheel pages years
                            onWheel: {
                                var step = (wheel.modifiers & Qt.ShiftModifier) ? 12 : 1
                                root.page((wheel.angleDelta.y > 0) ? -step : step)
                                wheel.accepted = true
                            }
                        }
                    }
                }

                // ══ EXPANDED: the lunisolar year — 3×4 solar blocks, lunar
                // noumenia marks, computed Attic month inscriptions ════════
                Column {
                    id: expandedBody
                    anchors.horizontalCenter: parent.horizontalCenter
                    y: Math.round(-40 * (1 - mode.frac) + root.pageShift)
                    spacing: 8
                    opacity: Math.max(0, mode.frac * 2.2 - 1.2)
                    visible: opacity > 0.01

                    Repeater {
                        model: 4
                        delegate: Row {
                            required property int index
                            readonly property int rowIdx: index
                            spacing: 12

                            Repeater {
                                model: 3
                                delegate: Item {
                                    id: block
                                    required property int index
                                    readonly property int m: parent.rowIdx * 3 + index
                                    readonly property var moons: root.monthMoons(root.viewYear, m)
                                    readonly property var phases: root.monthPhases(root.viewYear, m)
                                    readonly property int firstWd: new Date(root.viewYear, m, 1).getDay()
                                    readonly property int dcount: new Date(root.viewYear, m + 1, 0).getDate()
                                    readonly property bool isViewed: m === root.viewMonth
                                    readonly property bool holdsToday:
                                        root.viewYear === root.now.getFullYear() && m === root.now.getMonth()

                                    width: 177        // 16 week rail + 1 + 7×22 cells + 6×1
                                    height: 137       // name 16 + attic 14 + grid 101 + 2×3

                                    Column {
                                        anchors.fill: parent
                                        spacing: 3

                                        // Gregorian month name — gold marks the
                                        // month the compact view sits on
                                        Text {
                                            height: 16
                                            text: root.monthNames[block.m]
                                            color: block.isViewed ? root.theme.paletteAccent : root.ink
                                            font.family: root.faceSerif
                                            font.pixelSize: 13
                                            font.bold: true
                                        }

                                        // The Attic month BEGINNING at this
                                        // block's noumenia (computed) — aegean,
                                        // the lunar information layer
                                        Text {
                                            height: 14
                                            text: block.moons.length > 0 ? block.moons[0].name : ""
                                            color: root.withA(root.aegean, 0.85)
                                            font.family: root.faceSerif
                                            font.pixelSize: 11
                                            font.italic: true
                                        }

                                        // mini day grid — fixed 6×7, Sunday-first
                                        Column {
                                            spacing: 1      // 6×16 + 5×1 = 101
                                            Repeater {
                                                model: 6
                                                delegate: Row {
                                                    required property int index
                                                    readonly property int wIdx: index
                                                    spacing: 1

                                                    // ISO week rail — the compact
                                                    // gutter at mini scale, silent
                                                    // (opacity, not visible: the
                                                    // Row must hold its slot) on
                                                    // rows with no day of the month
                                                    Text {
                                                        width: 16
                                                        height: 16
                                                        horizontalAlignment: Text.AlignRight
                                                        verticalAlignment: Text.AlignVCenter
                                                        rightPadding: 3
                                                        opacity: (parent.wIdx * 7 - block.firstWd + 1 <= block.dcount) ? 1 : 0
                                                        text: root.isoWeek(root.viewYear, block.m,
                                                            parent.wIdx * 7 + 4 - block.firstWd + 1)
                                                        color: root.withA(root.ink, 0.32)
                                                        font.family: root.faceMono
                                                        font.pixelSize: 9
                                                    }

                                                    Repeater {
                                                        model: 7
                                                        delegate: Rectangle {
                                                            required property int index
                                                            readonly property int dayN:
                                                                parent.wIdx * 7 + index - block.firstWd + 1
                                                            readonly property bool inMonth:
                                                                dayN >= 1 && dayN <= block.dcount
                                                            readonly property bool isToday:
                                                                block.holdsToday && inMonth && dayN === root.today
                                                            // which phase (if any) marks this day
                                                            readonly property int phaseQ: {
                                                                if (!inMonth || isToday) return -1
                                                                for (var i = 0; i < block.phases.length; i++)
                                                                    if (block.phases[i].day === dayN)
                                                                        return block.phases[i].q
                                                                return -1
                                                            }
                                                            width: 22
                                                            height: 16
                                                            radius: 0
                                                            color: isToday ? root.theme.paletteHot : "transparent"
                                                            // numeral CENTERED: at 22×16 with an 8px moon this is the compact grid's proportion (26×17 / 12 / 9), and centering keeps a corner moon nearer its OWN numeral than the next one's
                                                            Text {
                                                                anchors.centerIn: parent
                                                                text: parent.inMonth ? parent.dayN : ""
                                                                color: parent.isToday ? root.theme.paletteBg
                                                                       : (index === 0 ? root.withA(root.clay, 0.8)
                                                                                      : root.withA(root.ink, 0.8))
                                                                font.family: root.faceSerif
                                                                font.pixelSize: 11
                                                                font.bold: parent.isToday
                                                            }
                                                            // the phase, corner-inscribed at mini
                                                            // scale — color emoji by sanction,
                                                            // aegean the monochrome-fallback ink
                                                            Text {
                                                                visible: parent.phaseQ >= 0
                                                                anchors.top: parent.top
                                                                anchors.right: parent.right
                                                                anchors.topMargin: -2
                                                                text: parent.phaseQ >= 0
                                                                      ? root.phaseGlyphs[parent.phaseQ] : ""
                                                                font.family: root.faceSerif
                                                                font.pixelSize: 8
                                                                color: root.aegean
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }

                                    // click a block → compact view of that month
                                    MouseArea {
                                        anchors.fill: parent
                                        cursorShape: Qt.PointingHandCursor
                                        onClicked: {
                                            root.monthOffset =
                                                (root.viewYear - root.now.getFullYear()) * 12
                                                + (block.m - root.now.getMonth())
                                            mode.toggle()
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // ── memento mori — two pieces, phrase answered by counter across the year's last line; lives in this body layer, so it can only ever exist in the expanded year ──────────────
                    Item {
                        width: 3 * 177 + 2 * 12  // the block rows' width — not
                                                 // parent.width (binding loop:
                                                 // the Column sizes from us)
                        height: 16
                        Text {
                            anchors.left: parent.left
                            anchors.bottom: parent.bottom
                            text: "memento mori"
                            font.family: root.faceSerif
                            font.pixelSize: 10
                            font.italic: true
                            color: root.withA(root.ink, 0.5)
                        }
                        Text {
                            anchors.right: parent.right
                            anchors.bottom: parent.bottom
                            text: root.daysLeft + " days · " + root.weeksLeft + " weeks left"
                            font.family: root.faceMono
                            font.pixelSize: 10
                            color: root.withA(root.clay, 0.9)
                        }
                    }
                }

                // wheel anywhere over the year pages ±1 year
                MouseArea {
                    anchors.fill: parent
                    acceptedButtons: Qt.NoButton
                    enabled: mode.expanded
                    onWheel: {
                        root.page((wheel.angleDelta.y > 0) ? -12 : 12)
                        wheel.accepted = true
                    }
                }
            }

            // ── Tally ledger: day count / distance / Metonic position ────
            Item {
                width: parent.width
                height: 18
                Rectangle {
                    anchors.top: parent.top
                    anchors.left: parent.left; anchors.right: parent.right
                    height: 1; color: root.withA(root.clay, 0.3)
                }
                Text {
                    anchors.left: parent.left
                    anchors.bottom: parent.bottom
                    text: root.tallyText()
                    color: root.theme.paletteAccent
                    font.family: root.faceMono
                    font.pixelSize: 11
                    font.bold: true
                }
                Text {
                    anchors.right: parent.right
                    anchors.bottom: parent.bottom
                    width: 96
                    horizontalAlignment: Text.AlignRight
                    text: root.kaomojiFor()
                    font.family: root.faceMono
                    font.pixelSize: 12
                    color: root.withA(root.clay, 0.9)
                }
            }

            // ── Closing rule — the score still ends on the 𝄂 barline ─────
            Item {
                width: parent.width
                height: 14
                Text {
                    id: closeBar
                    anchors.right: parent.right; anchors.rightMargin: 2
                    anchors.verticalCenter: parent.verticalCenter
                    text: "𝄂"
                    font.family: root.faceMusic; font.pixelSize: 16
                    color: root.theme.paletteAccent
                }
                Rectangle {
                    anchors.left: parent.left; anchors.right: closeBar.left
                    anchors.rightMargin: 6
                    anchors.verticalCenter: parent.verticalCenter
                    height: 1; color: root.withA(root.clay, 0.55)
                }
            }
        }
    }

    // ── The rollers — wound papyrus, Canvas linework: 2-turn spiral ends,
    // umbilicus axle + knob (cornua) protruding past them, tangent cylinder
    // lines, a faint wound-layer seam. Both lengthen with the mode morph;
    // the bottom one travels the unfurl and is a grab-handle for the toggle ─
    component Roller: Canvas {
        // Rollers lerp with the SHEET (painted morph), not the stepped
        // window — left-pinned with it, lengthening rightward as it grows.
        width: root.sheetW + 2 * root.protrusion
        height: 16
        onWidthChanged: requestPaint()
        onPaint: {
            var ctx = getContext("2d")
            ctx.reset()
            ctx.clearRect(0, 0, width, height)
            ctx.lineWidth = 1.3
            var midY = height / 2
            var r = 6.5
            var lcx = 11, rcx = width - 11
            ctx.strokeStyle = root.clay
            // end spirals — the rolled sheet seen end-on
            var ends = [lcx, rcx]
            for (var e = 0; e < 2; e++) {
                ctx.beginPath()
                var steps = 32
                for (var s = 0; s <= steps; s++) {
                    var t = s / steps
                    var ang = t * Math.PI * 4              // 2 turns
                    var rr = r * (1 - t * 0.92)
                    var x = ends[e] + rr * Math.cos(ang)
                    var y = midY + rr * Math.sin(ang) * 0.92
                    if (s === 0) ctx.moveTo(x, y); else ctx.lineTo(x, y)
                }
                ctx.stroke()
            }
            // umbilicus — the axle rod + end knob (cornua) past each spiral
            ctx.beginPath()
            ctx.moveTo(lcx - r, midY); ctx.lineTo(2.5, midY)
            ctx.moveTo(rcx + r, midY); ctx.lineTo(width - 2.5, midY)
            ctx.moveTo(2.5, midY - 3); ctx.lineTo(2.5, midY + 3)
            ctx.moveTo(width - 2.5, midY - 3); ctx.lineTo(width - 2.5, midY + 3)
            ctx.stroke()
            // cylinder silhouette — tangent to the spiral circles
            var off = 3.2
            var tx = Math.sqrt(r * r - off * off)
            ctx.beginPath()
            ctx.moveTo(lcx + tx, midY - off); ctx.lineTo(rcx - tx, midY - off)
            ctx.moveTo(lcx + tx, midY + off); ctx.lineTo(rcx - tx, midY + off)
            ctx.stroke()
            // wound-layer seam — one faint inner line
            ctx.strokeStyle = root.withA(root.clay, 0.3)
            ctx.beginPath()
            ctx.moveTo(lcx + r, midY); ctx.lineTo(rcx - r, midY)
            ctx.stroke()
        }
    }

    Roller { y: 0 }
    Roller {
        id: bottomRoller
        y: root.sheetY + root.sheetVisH - 1
        // the rod as a handle — pull it to unroll the year / roll it back
        MouseArea {
            anchors.fill: parent
            anchors.margins: -2
            cursorShape: Qt.PointingHandCursor
            onClicked: mode.toggle()
        }
    }
}
