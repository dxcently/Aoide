// calendar.qml — sonata/Greek-marble "calendar" flavor widget: the COMPOSITE
// order carried on a PAPYRUS SCROLL — a self-framed stele that unrolls.
//
// khoa, 2026-08-15 (round seven): the nav row was over-subscribed and the
// expanded year was under-sized — the mode control moves to a band that
// fits it, and the year's mini-grids grow to the compact grid's own
// proportions.
//
// ── Round-seven additions ───────────────────────────────────────────────
//
// THE TOGGLE LEAVES THE NAV ROW (overturns round six's placement AND its
// bracket-tag form; reason below). Measured on this font stack with
// TextMetrics, not estimated — Noto Serif 14 DemiBold: "September" 76.95,
// "November" 74.28, "December" 72.30; "2026" 31.63; "[ ἔτος ⌄ ]" 60.00 at
// JetBrainsMono NF 10. The compact nav row is 232 wide (256 sheet − 2×12).
// TEXT ALONE is 77 + 32 + 60 = 169 of that — 73% — leaving 63px for four
// arrow wrappers and every gap between three groups. At the shipped sizes
// monthCluster reached x≈111 while the parent-centered toggle spanned
// 86–146: 25px of overlapping PAINT, and their MouseAreas (−3 on the name,
// −5 on the toggle) overlapped by 33px, so a click near the seam was
// ambiguous — the next-month ‹ was drawn entirely UNDER the toggle (live
// capture). No arrangement of three groups clears 232: centered-in-the-gap
// wants 76 and the gap is 55; tightening both clusters AND right-anchoring
// the toggle still lands ~21px over. So the row now keeps only what it is
// FOR — month paging left, year paging right, 55px of clear air between
// them at the worst name — and the mode control takes the answer line's
// void, which was 60% empty in compact and emptier still in the year.
//
// IT IS NOW A DRAWN CHIP, not a [ token ]. Right-anchored as bare text it
// stacked under [ fasti ] as a second mono bracket label in the same hue
// (captured, rejected); the fix is SHAPE, not another position — a 62×20
// hairline box, radius 0, clay border 0.55 → 0.9 with a 0.16 clay fill on
// hover: notifications.qml's action-button idiom. [ token ] stays the
// grammar for a TAG (identity, no hit region); a control gets a pressable
// edge. Cost: the answer line 12 → 20, so the chip's hit box (62×20,
// MouseArea −5 → 72×30) is no smaller than the old bare text's 70×30 —
// chromeH 148 → 156 with it, the ONE compact number this round moves, and
// it moves for the hit target. The fixed chip width also means the
// ἔτος/μήν swap no longer changes the control's footprint mid-morph.
//
// THE EXPANDED YEAR SCALES UP (by direction — "the indv months are too
// small"). 7–8px type was below reading size. One coherent step, taken by
// moving the mini grid ONTO the compact grid's proportions rather than
// inventing new ones:
//     cell 15×11 → 22×16     numeral 8 → 11      phase mark 7 → 8
//     rail 12w/7px → 16w/9px                     block 124×97 → 177×137
//     Gregorian 11 → 13      Attic 9 → 11        memento 8 → 10
//     block gap 8 → 12       row gap 6 → 8       block inner spacing 2 → 3
//     expandedSheetW 412 → 579      expandedBodyH 424 → 596
//     expandedWinW  448 → 615       expandedWinH  586 → 784
// 615×784 hangs off the clock cell well inside 1920×1080 against the bar's
// 36px reserve (live-verified: layer surface at 40,40 measuring 621×792,
// nothing clipped). Compact is untouched by this half — 256/130 stand.
//
// NUMERALS RETURN TO CENTER (retires round four's left-anchoring, which was
// forced by the 15×11 cell and is no longer true at 22×16). With an 8px
// moon the mini cell is the compact 26×17 / 12 / 9 recipe to within a
// pixel, and centering is precisely what makes a corner moon read: kept
// left-anchored at the new width, the moon drifts to the middle of its own
// cell and lands nearer the NEXT day's numeral than its own (captured, then
// fixed). Round four's note is retired, not merely stale.
//
// khoa, 2026-08-13 (round six, same day, retained): the chrome turns with the
// material — controls that pointed sideways at a sheet that moves
// vertically now point along the roll.
//
// ── Round-six additions ─────────────────────────────────────────────────
//
// GUILLEMETS ROTATED, NOT REPLACED. The nav arrows were ‹ ›/« » — designed
// when paging swapped in place, horizontal glyphs over now-vertical
// motion. Recon over the alternatives (all render-tested at size): native
// serif wedges ˄ ˅ are modifier-letter small, superscript-floating — weak
// targets; the CJK vertical forms ︿ ﹀ ︽ ︾ are the true vertical
// guillemets but arrive sans-weight and full-width via Noto Sans CJK
// fallback, overwhelming the serif line. So the SAME glyphs rotate 90°:
// identical strokes, identical single=month/double=year and clay/hover
// distinction, zero new fallback dependence — the guillemets simply
// turned with the material. PREV POINTS UP (earlier days are wound into
// the top roller — and wheel-up already pages backward), next points
// down. Rotated Texts ride fixed wrapper Items, because a rotated Text's
// layout box does not rotate with it.
//
// THE TOGGLE TAG HINTS ITS DIRECTION: [ ἔτος ⌄ ] unrolls downward,
// [ μήν ⌃ ] rolls back up — mono wedges U+2303/2304, native in
// JetBrainsMono NF. [ROUND SEVEN: the wedges and their direction hold, but
// the tag left this row for the answer line and became a drawn chip — the
// row could not hold three groups. See the round-seven block above.]
// The bottom roller stays the wordless handle. The
// rest of the chrome was audited and left alone: the wheels are
// invisible, the block-click's motion is the mode morph, the tally's
// "wound ±" is temporal not spatial, and the rollers/frieze/𝄂 are
// physics and ornament, not controls.
//
// khoa, 2026-08-13 (round five, same day, retained): paging learns what
// the toggle already knew — a scroll MOVES. Month and year paging now
// unscroll vertically instead of swapping in place.
//
// ── Round-five additions ────────────────────────────────────────────────
//
// PAGING = UNSCROLLING. Every paging gesture (‹ ›, « », all three wheels,
// the month/year home-clicks) routes through page(delta), which turns the
// data swap into material motion on the SAME body-layer slide idiom as
// the mode morph: grabToImage snapshots the body's current face into
// pageGhost (one Image — no duplicated delegates, no second model), the
// data swaps beneath it, and a single pageShift slide moves ghost and
// live layer in lockstep through bodyRegion's clip — forward in time
// pulls new days up from the bottom roll, backward rolls them down from
// the top, matching the hung-rotulus physics of the open-unfurl. 220ms
// OutCubic, painted motion only — the window never moves (sizing
// discipline untouched). Re-paging mid-slide swaps data under the running
// motion (a fast wheel reads as fast scrolling, never queues); paging
// mid-morph or a failed grab degrades to the old instant swap. The
// expanded block-click keeps its direct jump — the mode morph is that
// transition's motion.
//
// khoa, 2026-08-13 (round four, same day, retained): the expanded year
// grows a spine and a memento — numerals return beside the moons, ISO
// weeks rail every mini-grid, and the closing barline counts what
// remains of the year.
//
// ── Round-four additions ────────────────────────────────────────────────
//
// NUMERAL + MOON TOGETHER (expanded). Round three's replace-the-numeral
// treatment is retired by direction: every mini-cell keeps its day number
// and phase days carry the emoji as a 7px top-right corner mark — the
// compact grid's almanac idiom at mini scale. At 15×11 the two only
// coexist off-axis, so mini-grid numerals sit LEFT-ANCHORED (column
// alignment held; a centered digit puts its shoulder under the moon).
// [ROUND SEVEN: retired. The cell is 22×16 with an 8px moon now, and at
// that size left-anchoring is what breaks the read — numerals are centered
// again. See the round-seven block above.]
//
// WEEK RAIL (expanded). Each month block gains the compact gutter's ISO
// weeks — dim mono numerals down a 12px rail, numbered by each row's
// Thursday (same Sunday-first apology as round two), silenced by opacity
// (never `visible:` — the Row must hold the slot) on rows carrying no day
// of the month. Costs width: expandedSheetW 390 → 412, block spacing
// 14 → 8 — the one geometry move this round; the toggle still steps
// between per-mode constants, discipline intact.
//
// MEMENTO MORI (expanded only, by direction — the memento belongs to the
// year laid out whole, not the working month). Two DISTINCT pieces, not a
// sentence: the phrase "memento mori" in the ἡμερολόγιον answer-line
// italic at the year's lower left, answered across the line by the
// counter — "N days · M weeks left", mono figures in dim clay — at the
// lower right. The row lives INSIDE the expanded body layer (not the
// chrome), so it arrives and leaves with the year morph for free and
// cannot exist in compact; expandedBodyH 406 → 424 pays for it. Counted
// from the REAL today (root.now → Dec 31), not the browsed year — a
// memento reads your days, not the scroll's. The closing 𝄂 rule stays an
// empty stave; the Metonic tally stays untouched.
//
// khoa, 2026-08-13 (round three, same day, retained): the moon appears — all
// EIGHT phases, drawn from the SAME mean-lunation lattice the Attic year
// already runs on, inscribed as the real color moon-phase emoji.
//
// ── Round-three additions ───────────────────────────────────────────────
//
// PHASE GLYPHS (U+1F311–1F318, EMOJI presentation — a SANCTIONED COLOR
// EXCEPTION). The first pass chose text presentation (+U+FE0E): the moon
// emoji are absent from all three declared faces (fc-list verified), bare
// they fall back to Noto Color Emoji — fixed-color, deaf to `color:` —
// while FE0E resolves them to DejaVu Sans monochrome, which recolors in
// notes.* ink and stays legible at 9px (all render-verified side-by-side,
// including against ●◐○◑ and the Nerd Font PUA weather moons). The owner
// then overrode, explicitly: real emoji. So the phase marks are the ONE
// glyph family wearing fixed color outside the notes.* roles — scoped
// strictly to this family, everything else on the sheet still inks from
// notes.*. Should color emoji ever leave the rig, the FE0E monochrome set
// (aegean, ink-is-shadow polarity) is the documented fallback.
//
// EIGHTHS. monthPhases() divides each lunation at k + q/8, q 0..7 — new,
// waxing crescent, first quarter, waxing gibbous, full, waning gibbous,
// last quarter, waning crescent — the same Meeus mean lattice, so q0
// still lands exactly where the Attic months open, and the same ±1-day
// honesty as the noumeniai (the intermediate eighths are midpoints of
// mean anomaly, not almanac events; day-level is all they claim).
//
// PLACEMENT. A mark lands roughly every 3.7 days (~8 a month). Compact
// grid (26×17 cells): every phase day carries the emoji in the cell's
// top-right corner; the numeral KEEPS its role ink — with 8 phases a
// tinted numeral would drown the Sunday rubric, the emoji alone is the
// mark, and the round-two noumenia box stays retired (noumenia IS the new
// moon; the mark now shows the phase). Expanded mini-grids (15×11 cells):
// the emoji REPLACES the numeral on phase days (grid position still dates
// it; corner marks don't fit at that size). Today's laurel outranks a
// phase mark in both modes, as it outranked the box.
//
// khoa, 2026-08-13 (round two, same day, retained): the scroll learns to unroll FURTHER —
// a compact month view and an expanded LUNISOLAR year view share one sheet.
// Round-one architecture unchanged: self-framed stele hosted bare via
// StelePopout (the 2026-07-31 standing direction; GadgetFrame's bay retired
// for this popout, Π order-mark dormant like Α Β Γ Δ Θ Ω before it).
//
// The order (held; see round-one header block below for the full account):
//   · ORDER COMPOSITE — the volute order; voluta = "rolled", the scroll in
//     stone. · SIGNATURE clay `notes.base09` as the RUBRIC (red-ochre ink of
//     the fasti; the "unclaimed slot" claim stays DEAD — AudioColonnade and
//     UsageGadget also wear base09, collision still open for adjudication).
//     · CROWN 𝄴. · FRIEZE Vitruvian wave-scroll. · RUBRIC Sunday Κ + Sunday
//     numerals in clay. · LAUREL today's cell only.
//
// ── Round-two additions ─────────────────────────────────────────────────
//
// WEEK NUMBERS (compact grid gutter). ISO 8601 numbering — BUT the grid stays
// Sunday-first: the Greek weekday names themselves are Sunday-first ordinals
// (Δευτέρα/Τρίτη/Τετάρτη/Πέμπτη = "2nd/3rd/4th/5th day", Κυριακή being day
// one), so a Monday-first grid would contradict the very letters heading it.
// Each row is numbered by its THURSDAY's ISO week — self-consistent, since
// ISO 8601 defines a week's number AS its Thursday's week; on a Sunday-first
// row that covers 6 of 7 days (only the leading Sunday belongs to the prior
// ISO week). Dim caption ink; the row holding today sits a step brighter.
//
// GRANDER. Larger stele (sheet 244→256 wide, day cells 24×16→26×17, margins
// 12), crown 𝄴 grown to 24px, KALENDAE at 15px/looser tracking answered by a
// small italic ἡμερολόγιον (the AoidePanel "AOIDE ⁄ ᾠδή" bilingual-inscription
// idiom), and the rollers gain their UMBILICUS: the axle rod + end knob
// (cornua) real scrolls wound onto, protruding past the spiral ends.
//
// THE EXPANDED LUNISOLAR YEAR (toggle). The Attic civil calendar was
// lunisolar: 12 lunar months beginning at each noumenia (new-moon day), the
// year opening at the first new moon after the summer solstice, kept against
// the sun by intercalating a second Poseideon (Ποσειδεών β′) in embolismic
// years — regularized by Meton of Athens' 19-year cycle (432 BC), whose
// "golden number" survives in the computus. The expanded sheet is that
// structure made layout: a SOLAR frame (3×4 Gregorian month blocks) carrying
// LUNAR content — each block marks its computed noumenia day(s) with an
// aegean box (holoBlue = the information role; the moon over the Aegean) and
// inscribes the Attic month that BEGINS there (computed, not decorative: the
// labels roll over at the real lunations, Ποσειδεών β′ appears of its own
// accord in embolismic years, and Ἑκατομβαιών lands mid-summer). The footer
// tally reads the Metonic position ("meton N of 19 · 12/13 months").
// Compact mode carries the same noumenia box on its big grid.
//   Moon arithmetic is MEAN-LUNATION (Meeus: JDE 2451550.09766 + 29.530588861k)
//   — day-level, ±1 day vs true phase (verified against the 2026 lunations:
//   11 of 12 exact, Aug one day late). A rice widget, not an ephemeris.
//   Solstice approximated June 21. Noumenia dated in UTC.
//   Toggle affordances: the [ ἔτος ]/[ μήν ] tag centered in the nav row, or
//   grab the BOTTOM ROLLER itself (the rod you'd pull to unroll more).
//   Clicking a month block in the expanded year jumps the compact view to it.
//
// BOTH ROLLERS ANIMATE on the toggle, and the content moves with them. One
// `modeFrac` drives the whole morph through the SAME pipeline as the open-
// unfurl: sheet width and target height interpolate (both rollers visibly
// lengthen; the bottom one travels), and the two body layers slide/fade
// under the sheet's clip — compact rising into the top roll as the year
// unrolls in beneath it, reversed on re-roll — so it reads as material
// feeding through the rolls, not a panel resize. The open-unfurl itself is
// unchanged (top pinned under the bar, bottom unrolls — a hung rotulus).
//
// ── Sizing discipline (amended, deliberately) ───────────────────────────
// implicitWidth/implicitHeight are constants WITHIN a mode — month/year
// paging never resizes the window, and the open-unfurl animates painted
// heights only. The compact↔expanded toggle is the one sanctioned window
// resize, and it STEPS rather than animates the window while every visible
// surface (sheet, rollers, content) animates as paint. SteleLayerPopout
// tracks the stepped implicit sizes through its live bindings (body.width/
// implicitHeight via WidgetSlot).
//
// khoa, 2026-08-16 — THE 300ms resizeGuard IS RETIRED, and the morph now
// runs on the shared MorphState (facet). The guard was written against a
// recorded Hyprland behaviour — an xdg_popup re-mapped on the first resize
// of a visible popup, playing its popup animation (~0.25s vanish/fade) over
// the remap. Two things have changed under it. This popout has not been an
// xdg_popup since 2026-08-13 (it moved to its own layer surface for the
// crisp day grid), and layer surfaces are anchored, not re-mapped. And the
// behaviour itself no longer reproduces: burst-captured live at ~60ms/frame
// on Hyprland 0.56.0, this scroll expanded and collapsed with the surface
// never leaving `hyprctl layers`, its x/y pinned at 31,26 through a 298→621
// step, and every frame fully painted; the same probe run on a real
// xdg_popup (AudioColonnade) resized 8 times in 120 frames with ZERO absent
// frames. See MorphState.qml's header for the full capture.
// What survives the guard is the ORDER, and for a different reason —
// window OUT first, window IN last keeps the surface ≥ the paint at every
// instant, so the sheet's own border and cast shadow are never clipped
// mid-morph. That order is now the primitive's stepOut/stepIn hooks; the
// dead 300ms interval is gone, so expanding is that much quicker.
//
// khoa, 2026-08-13 (round one, retained): self-framed stele per StelePopout's
// standing direction after live recon confirmed genuine double-framing under
// GadgetFrame (two captions, two closures, glass losing the numerals over
// dark windows). Fixed injected-prop contract: `notes` + `bridge` only
// (bridge unused). Qt's Window.visible attached NEVER flips under
// quickshell's proxy windows (verified); QsWindow.window.visible is the real
// open/close edge. [2026-08-14: greek-grammar.md's §4 widget-by-widget map
// was cut outright in a retcon pass — there's no longer a shared section to
// carry the Composite/scroll row or the lunisolar expansion into; this
// header is this widget's design record now. The Π dormancy this note
// flagged is folded into greek-grammar.md §1 as part of that same pass.]

import QtQuick
import Quickshell   // QsWindow attached — the open/close edge (see header)
import "../.."      // the facet's shared components — MorphState (bar.qml idiom)

Item {
    id: root

    required property var notes
    required property var bridge

    readonly property string faceSerif: "Noto Serif"
    readonly property string faceMono:  "JetBrainsMono Nerd Font"
    readonly property string faceMusic: "Noto Music"
    readonly property color clay: notes.base09
    readonly property color ink: notes.paletteFg
    readonly property color aegean: notes.holoBlue   // the moon's information hue

    function withA(cstr, a) {
        var c = Qt.color(cstr)
        return Qt.rgba(c.r, c.g, c.b, a)
    }

    // ── Mode: compact month ↔ expanded lunisolar year ────────────────────
    // The state, the 0→1 clock and the ORDER around the window step all live
    // in the shared MorphState (facet-owned; see its header for the capture
    // that retired this file's 300ms resizeGuard). What stays here is the one
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
    // Paging UNSCROLLS (round-five note): page() grabs the body's current
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
    // the visible L only (right + bottom strips), NOT the old full shifted
    // rect: that rect underlapped the whole sheet, and now that the sheet
    // carries a transparent window over the day grid, an underlapping
    // shadow would tint the window's clear view of the desktop (khoa,
    // 2026-08-13, the opacity correction pass). Union is pixel-identical
    // to what the old rect showed around an opaque sheet ─────────────────
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

    // ── The sheet — fully opaque papyrus again (khoa, 2026-08-13, second
    // correction the same day: the transparent window over bodyRegion made
    // the day-grid numerals compete with whatever sat behind the popup —
    // live read was "the middle section where the numbers are should be
    // opaque". Back to a flat paletteBg fill, matching every other stele in
    // this family. The SteleLayerPopout hosting change (its own
    // "aoide-calendar" layer namespace, escaping aoide-bar's blur_popups
    // rule) stays — that was a real fix independent of this fill question,
    // it just no longer has anything to prove through a hole) ────────────
    Rectangle {
        id: sheet
        x: root.protrusion
        y: root.sheetY
        width: root.sheetW
        height: root.sheetVisH
        radius: 0
        color: root.notes.paletteBg
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
            // and, since round seven, the MODE LINE: the inscription names
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

                // The guillemets TURNED WITH THE MATERIAL (round-six note):
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
                        color: root.notes.paletteAccent
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
                // page turn reads as continuous material (round-five note)
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
                                            color: isToday ? root.notes.paletteHot : "transparent"

                                            Text {
                                                anchors.centerIn: parent
                                                text: parent.dayNum
                                                color: parent.isToday ? root.notes.paletteBg
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
                                            color: block.isViewed ? root.notes.paletteAccent : root.ink
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
                                                            color: isToday ? root.notes.paletteHot : "transparent"
                                                            // numeral CENTERED — round four's
                                                            // left-anchoring was forced by the old
                                                            // 15×11 cell; at 22×16 with an 8px moon
                                                            // this is the compact grid's proven
                                                            // proportion (26×17 / 12 / 9), and
                                                            // centering is what keeps a corner moon
                                                            // nearer its OWN numeral than the next
                                                            // one's (round-seven header note)
                                                            Text {
                                                                anchors.centerIn: parent
                                                                text: parent.inMonth ? parent.dayN : ""
                                                                color: parent.isToday ? root.notes.paletteBg
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

                    // ── memento mori — two pieces, phrase answered by
                    // counter across the year's last line (round-four
                    // header note); lives in this body layer, so it can
                    // only ever exist in the expanded year ──────────────
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
                    color: root.notes.paletteAccent
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
                    color: root.notes.paletteAccent
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
