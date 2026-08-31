// bar.qml — sonata's "bar" slot (was AoideBar.qml, moved out of the facet
// as of the widget-slot expansion, CONTRACTS.md §5). THE MEASURE — a
// clean-slate redesign in the SONG vein.
//
// Whole-content slot, hosted by `WidgetSlot`, not `SurfaceSlot` — the bar's
// `PanelWindow` stays in the facet (shell.qml); only its CONTENT is this
// per-song widget. The song fully owns its own footprint: the facet reads
// this file's `implicitHeight` back and binds the `PanelWindow`'s own
// `implicitHeight`/`exclusiveZone` to it (see shell.qml), rather than the
// facet pinning a fixed height. Extras this file needs beyond the universal
// `livery`/`bridge`: `stagingEngine` (so its own embedded calendar
// `WidgetSlot` can resolve), `powermenu` (the powermenu slot's live
// `.item`, for the clef), `shared` (session state, optional).
// `WorkspaceRow.qml` travels alongside this file as a helper component
// (uppercase filename — carried by the build, never independently
// resolvable as a slot), joined by `BarPopout`/`AudioColonnade` as of P1
// and `StelePopout`/`SteleLayerPopout` as of P2. `WidgetSlot` alone stays
// in the facet as shared, reusable chrome — reached via the
// `import "../.."` below.
//
// Aoide is the muse of song, so the bar is one bar of music. The old
// architectural grammar (the Pantheon entablature — keystone, colonnade wings,
// architrave lintel, floating BarPane volumes) is DISCARDED wholesale. Nothing
// of that metaphor survives. In its place: a single manuscript strip on which a
// five-line staff runs the full width of the screen, and every functional cell
// is written onto that staff as notation.
//
//     ╭─ 𝄞 ── ✎ ♫  title ─┃─ ●─●─┼─●─● ─┃─ ♫vol 𝄾bat 𝆹link ─ 𝄂 ─╮
//
//   HEAD      : 𝄞 treble clef — the key of the piece AND the powermenu key
//               (click → powermenu). It opens the staff.
//   LEFT      : ✎N agent-sessions (blocks → glitchPink pulse, a shipped tell),
//               the clock/date readout, and the active-window title
//               (music-kaomoji when empty).
//   CENTRE    : the workspaces, written as NOTE-HEADS on the staff line
//               (WorkspaceRow) — the melody of the measure, flanked by barlines.
//   RIGHT     : the expression marks — ♫ volume, 𝄾 battery (rests: the battery
//               empties into silence), 𝆹 network link — each hover/scroll/click
//               live, with popouts — then a system tray: a fermata toggle
//               (𝄐 closed / 𝄑 open — the hold, turned toward the popout)
//               that pops the held SNI items as a self-framed stele
//               (StelePopout) under the strip. Closed by a final barline 𝄂.
//
// Drawn structure (staff lines, barlines, playhead) carries the geometry; glyphs
// (clef, rests, note-marks) carry the ornament. It should read as sheet music.
//
// Geometry: `stripHeight` (36) is this song's own choice, read back by the
// facet rather than pinned by it (see the widget-slot note above) — the
// number happens to stay 36 for now, but the MECHANISM is dynamic, so a
// future song's `bar.qml` is free to pick a different footprint. Everything
// is drawn INSIDE the strip — no apron, no taller transparent surface (that
// scar, the wallpaper-bleed under a hairline, stays closed); the clef is
// sized to fit.
//
// Colour: the MUSIC SHEET — rendered as a flat, fully OPAQUE strip of
// manuscript paper, no glass and no gloss. The page fill is livery.paletteBg
// at full alpha (Qt.rgba(paletteBg.r, .g, .b, 1.0)); there is no blur behind
// it and no gradient sheen on top — the strip reads as solid paper. The
// structural ink (staff lines, barlines, page rail) follows livery.barFg,
// not a literal — sonata's own bar.fg (#2f2a33) sits near-black on the
// marble page, so this reads the same as the old #000000 did here, but a
// song that borrows this widget carries its own bar foreground with it
// (fugue's bar.fg is null, falling through to paletteFg, its bone-white
// #e4e6eb — the staff stays legible on graphite instead of vanishing). The
// TEXT ink is the song's umber (livery.paletteFg #423420) drawn CLEAN — the
// old white legibility outline is dropped, since dark text on the opaque
// sheet needs no halo (that outline was a relic of the old dark bar and
// only muddied the type on cream).
// One restrained STATE accent survives from the song (LiveryState): the ACTIVE
// workspace note-head fills with paletteAccent, the BLOCKED ✎ pulse + low battery
// go glitchPink, and open/hover toggles (volume) flash paletteAccent.
// Everything at rest follows livery.barFg.
//
// khoa, 2026-08-15 — THE AUDIO CELLS SPLIT IN TWO. Hovering an audio cell used
// to open the whole colonnade; it now opens a small CUE stele (the `AudioCue`
// component below) reading out output volume, mic status and bluetooth status,
// and nothing more. The colonnade moved to CLICK, latched on `audioOpen` — the
// same toggle idiom the tray fermata already uses on this strip. Four
// consequences, all decided rather than inherited:
//   · MUTE moved to MIDDLE-CLICK on either audio cell, since left-click is
//     now "open". Not a right-click context menu (nothing on this strip has
//     one, and one command does not justify inventing that system) and not
//     "it's inside the colonnade" alone (mute is an everyday action; making
//     it cost a surface plus two clicks is a regression). The cue prints
//     `mmb · mute` in its own ledger, so the gesture is on screen exactly
//     when the pointer is on the cell. The colonnade's per-column click
//     still mutes as well — that path is untouched.
//   · BLUETOOTH is now a real seam here (`Quickshell.Bluetooth`) and feeds
//     the colonnade's third bay. Grounding, and why the profile write is the
//     one thing that leaves QML, is on the seam itself.
//   · The CUE is a self-framed stele hosted bare through StelePopout, not the
//     BarPopout/GadgetFrame the battery gauge one cell over still wears —
//     that host is documented as legacy, "not the default for something new"
//     (widget-structure.md §4), and wrapping a self-framed stele in it
//     double-frames (live-confirmed on the calendar). It is the precedent for
//     the BEHAVIOUR of a small hover readout, not for its chrome.
//   · `[ mixer ]` in the colonnade's ledger launches pavucontrol through
//     `Quickshell.execDetached` (AoideClipboard's argv-only idiom). The
//     package is added by the quickshell facet — it is that facet's widget
//     that needs it.
//
// khoa, 2026-08-16 — THE CUE BECOMES A CONTROL, AND THE PILLARS GET ROSTERS.
// Three changes, all of them reversing or extending a decision from the day
// before:
//   · THE CUE IS REACHABLE. The 2026-08-15 build shed `audioBodyHover`, so
//     `audioCueShown` read the BAR CELL only and the card vanished the instant
//     the pointer left the cell — it could be looked at and never entered.
//     Restored as two latches (`cueBodyHover` for the card's own body,
//     `cueHotRow` for the channel rows) behind a 220ms grace `Timer`. ONE
//     `MouseArea` at the cue's root feeds both: anything that accepts hover
//     there was measured swallowing every nested row's hover, and a
//     `HoverHandler` did not compose any better, so the row band is resolved
//     ARITHMETICALLY from the pointer position instead. Its
//     `anchors.topMargin: -4` reaches past the stele to claim StelePopout's
//     4px host gap — between the cell's bottom edge and the stele's top edge
//     lie 4px of popup surface owned by no item, and a pointer that pauses
//     there outlives any grace. Both livery are recorded on the component
//     itself with what was measured.
//   · MUTE MOVES INTO THE CUE, AND MIDDLE-CLICK IS WITHDRAWN. Each channel row
//     is now clickable — out and in toggle mute, bt toggles adapter power
//     (this file's own colonnade already holds that powered-down and silenced
//     are one idea and draws them as one shape). Yesterday's middle-click on
//     the bar cell existed for exactly one reason — left-click had just been
//     taken by "open" and mute could not be allowed to cost a surface plus
//     two clicks. It no longer costs that: the cue is already on screen
//     whenever the pointer is on the cell, it now STAYS on screen, and one
//     click on the row does it, for all THREE channels instead of two. A second
//     invisible path to a command that is now drawn on screen is not a shortcut,
//     it is a thing that rots, so `Qt.MiddleButton` is gone from both cells
//     and `mmb · mute` is gone from the ledger with it — the line now reads
//     `click · silence`, which describes a gesture the surface it is printed
//     on actually has.
//   · THE COLONNADE'S PILLARS GET ROSTERS. Each bay carries a plinth chip
//     naming its current channel, and the chip opens a picker over the porch.
//     VOL lists sinks, MIC lists sources, BT lists bluez devices. Grounded on
//     the compiled quickshell-service-pipewire.qmltypes: `defaultAudioSink`
//     and `defaultAudioSource` are `isReadonly: true`, but the file also
//     declares `preferredDefaultAudioSink` / `preferredDefaultAudioSource`
//     with `write: "setDefaultConfiguredAudioSink"` /
//     `"setDefaultConfiguredAudioSource"` — i.e. the default IS settable
//     natively, and no `pactl set-default-sink` shell-out is needed. That is
//     the whole switch. The roster itself comes off `Pipewire.nodes`, filtered
//     by `PwNodeType.Audio` with `isStream` excluded, and the BT roster off
//     `Bluetooth.devices` with `connect()`/`disconnect()` (the only commands that
//     module has — see the 2026-08-15 grounding on the seam below).
//
// Data sources (unchanged real Quickshell services — the plumbing survives):
//   - Hyprland   → workspaces (WorkspaceRow) + active window title.
//   - Pipewire   → default sink/source volume / muted, and the bluez node's
//                  own `api.bluez5.profile` property (the profile READ).
//   - Bluetooth  → bluez adapter power + connected device (Quickshell 0.3.0).
//   - UPower     → display-device battery.
//   - Network    → /proc/net/route via FileView (files-not-processes rule)
//                  for the GLYPH; Quickshell.Networking (native NM binding)
//                  for the DIKTYON picker stele — scan/join/forget/PSK.
//   - SystemTray → StatusNotifier items (the collapsible tray); rows open
//                  their dbusmenu via QsMenuAnchor (needs the shell.qml
//                  UseQApplication pragma — see hazards.md §4).
//   - Sessions   → state/stage/sessions.json + hooks.json via FileView.

import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import Quickshell.Hyprland
import Quickshell.Bluetooth
import Quickshell.Services.Pipewire
import Quickshell.Services.UPower
import Quickshell.Services.SystemTray
import Quickshell.Networking
// Reaches the facet's shared, reusable `WidgetSlot` — none of it is
// bar-specific content, so it stays in the facet rather than moving here.
// BarPopout and AudioColonnade moved to sonata/widgets/ in P1, StelePopout
// and SteleLayerPopout in P2, and all four resolve same-dir now.
// Deployed-tree relative path: this file lands at $out/qml/songs/sonata/
// bar.qml, so two levels up ($out/qml/songs/ → $out/qml/) is the facet's
// own qml/ root (modules/facets/quickshell/default.nix's build walk fixes
// that depth by construction — see its own comment on the songs/ copy).
import "../.."

Item {
    id: root

    // ── WorkspaceRow — inlined, not a sibling file ──────────────────────────
    // Was WorkspaceRow.qml, a sibling helper file. Quickshell's dynamic
    // Component.createObject(url) loading (how WidgetSlot loads this whole
    // bar.qml) does not reliably grant a loaded file visibility into custom
    // types in its own directory — neither the implicit same-directory rule
    // nor a top-level file-scoped `component` (that specifically hit "Syntax
    // error" here, unlike GlassPage's NESTED placement below, which is why
    // this is nested the same way GlassPage is in launcher.qml, not floated
    // above the root the way a first attempt at this tried) resolved it
    // (confirmed live: calendar.qml, which references no custom sibling
    // type, loads fine through the identical path; bar.qml failed with
    // "WorkspaceRow is not a type" either way it was tried). Declared as a
    // locally-nested `component` instead — sidesteps cross-file resolution
    // entirely since it's just one file being read, and matches the one
    // proven-working precedent in this codebase.
    //
    // The melody: workspaces as SOLID NOTE GLYPHS on the staff. Clean-slate
// redesign in the SONG vein (the entablature colonnade is gone). The bar is
// one measure of music; the workspaces are the livery written on it. Each
// live Hyprland workspace is a SOLID (filled) musical note, one distinct
// glyph per workspace id:
//     ws 1 → ♩   ws 2 → ♪   ws 3 → ♫   ws 4 → ♬   ws 5 → 𝅘𝅥𝅮   ws 6 → 𝅘𝅥𝅯
//     (ids past the set repeat the run; magic → 𝅘𝅥𝅱, scratch → 𝄋 ride the ledger)
// Pitch (staff degree) rises with id. The SELECTED workspace swells, fills
// with its own hue, and rests on a soft accent highlight-pill. Resting livery
// each carry their own hue (livery.noteColor cycles the 8-slot base16 accent
// spread); urgent workspaces PULSE in glitchPink, overriding the resting hue.
// Clicking a note activates its workspace (Hyprland activate(), no shell-out).
// Colour comes ONLY from the livery singleton, save one sanctioned literal:
// the white outline on the selected glyph, for separation.
component WorkspaceRow: Item {
    id: root
    required property var livery

    // ── Hover-preview bridge (concepts/Terminal-Commander) ─────────────────
    // shell.qml's shared QtObject. When the gadget-dock terminal roster is
    // hovered, TerminalManagerGadget writes that session's Hyprland workspace id
    // to `shared.hoveredWorkspace`; we render a DISTINCT preview highlight on the
    // matching note glyph — a cool holoBlue RING (+ glyph tint), clearly NOT the
    // warm accent swell+pill of the truly-active workspace. -1 = nothing hovered.
    // Optional/null-safe: hosted without `shared`, the preview simply never fires.
    property var shared: null
    readonly property int hoveredWs: shared ? shared.hoveredWorkspace : -1
    // Index in wsList of the previewed workspace (-1 when none / not live). No
    // real Hyprland workspace carries id -1, so the sentinel naturally misses.
    readonly property int hoveredIndex: {
        var vs = root.wsList
        for (var i = 0; i < vs.length; i++)
            if (vs[i] && vs[i].id === root.hoveredWs) return i
        return -1
    }

    // ── Geometry: solid livery on a five-line staff ─────────────────────────
    readonly property int cellW: 22        // horizontal slot per note
    readonly property int cellGap: 3
    readonly property real halfStep: 1.6   // line→space; a full staff line is 2×
    readonly property int restSize: 15     // resting note glyph
    readonly property int activeSize: 19   // the played note swells

    implicitWidth: cellRow.implicitWidth
    implicitHeight: 30

    // ── The solid-note vocabulary — DISTINCT filled glyph per workspace id ──
    // All heads are SOLID (filled); the run repeats past its length. Specials
    // keep their own marks and perch above the staff on a ledger.
    readonly property var noteGlyphs: ["♩", "♪", "♫", "♬", "𝅘𝅥𝅮", "𝅘𝅥𝅯"]
    function wsGlyph(ws) {
        if (!ws) return "♩"
        var name = ("" + (ws.name || "")).toLowerCase()
        if (name.indexOf("magic") !== -1) return "𝅘𝅥𝅱"
        if (name.indexOf("scratch") !== -1) return "𝄋"
        var id = ws.id || 1
        return root.noteGlyphs[(id - 1) % root.noteGlyphs.length]
    }
    function wsSpecial(ws) {
        var name = ws ? ("" + (ws.name || "")).toLowerCase() : ""
        return name.indexOf("magic") !== -1 || name.indexOf("scratch") !== -1
    }
    // Resting-state hue — each workspace id gets its own distinct base16 accent
    // (livery.noteColor cycles the 8-hue accent spread by id). Active/urgent/
    // preview states still override this in the delegate below.
    function wsColor(ws) {
        var id = ws ? (ws.id || 1) : 1
        return root.livery.noteColor(id)
    }
    // Staff degree → vertical offset (up is negative y). Specials ride high on
    // a ledger; regular ids wrap through a 7-degree scale centred on the staff.
    function pitchOffset(ws) {
        if (wsSpecial(ws)) return -5 * root.halfStep          // above the top line
        var id = ws ? (ws.id || 1) : 1
        var deg = ((id - 1) % 7) - 3                          // -3 … +3
        return -deg * root.halfStep
    }

    // Sorted live workspaces (by id) — ObjectModel exposes `.values`.
    readonly property var wsList: {
        var vs = (Hyprland.workspaces && Hyprland.workspaces.values)
                 ? Hyprland.workspaces.values.slice() : []
        vs.sort(function (a, b) { return (a.id || 0) - (b.id || 0) })
        return vs
    }

    // Index of the focused note (drives the highlight-pill); -1 when none.
    readonly property int activeIndex: {
        var vs = root.wsList
        for (var i = 0; i < vs.length; i++)
            if (vs[i] && (vs[i].active || vs[i].focused)) return i
        return -1
    }

    // Selected-workspace colour = that workspace's OWN per-id hue (not a uniform
    // gold), so the highlight-pill and the swelled note read in the workspace's
    // own colour, just emphasised. Falls back to the accent when nothing's active.
    readonly property color activeColor:
        (root.activeIndex >= 0 && root.wsList[root.activeIndex])
        ? root.livery.noteColor(root.wsList[root.activeIndex].id)
        : root.livery.paletteAccent

    // ── The highlight-pill — a soft accent glow that eases under the played
    // note (the old playhead re-cast). Declared before the livery so it sits
    // behind them, reinforcing which workspace is selected.
    //
    // khoa, 2026-08-16: verticalCenterOffset added — every note glyph is
    // shifted off WorkspaceRow's own centre by pitchOffset(ws) (its staff
    // degree; "Pitch (staff degree) rises with id" above), but this mark
    // had none, so it only ever lined up with whichever workspace's id
    // happened to resolve pitchOffset to 0 and visibly drifted off every
    // other note (confirmed live: a circle centred a full staff-space above
    // where the actual glyph sat). Matching the SAME offset the active
    // note's own glyph uses keeps the two locked together regardless of id.
    Rectangle {
        id: highlight
        visible: root.activeIndex >= 0
        z: 0
        width: 22
        height: 22
        // Equal width/height so radius: width/2 rounds both axes fully —
        // a true circle, not a stadium-shaped pill.
        radius: width / 2
        color: root.activeColor
        opacity: 0.20
        anchors.verticalCenter: parent.verticalCenter
        anchors.verticalCenterOffset:
            root.activeIndex >= 0 ? root.pitchOffset(root.wsList[root.activeIndex]) : 0
        x: root.activeIndex >= 0
           ? root.activeIndex * (root.cellW + root.cellGap) + (root.cellW - width) / 2
           : 0
        Behavior on x {
            NumberAnimation { duration: 180; easing.type: Easing.OutCubic }
        }
        Behavior on anchors.verticalCenterOffset {
            NumberAnimation { duration: 180; easing.type: Easing.OutCubic }
        }
    }

    // ── The preview RING — the hover-preview highlight (distinct from active).
    // A hollow holoBlue outline that eases under the workspace the hovered
    // terminal lives on. Deliberately a DIFFERENT KIND of mark from the active
    // pill (hollow cool ring vs solid warm accent fill), so both can show at
    // once — if the previewed ws IS the active one, the ring simply frames the
    // accent pill and reads sensibly. Border-only, input-inert. Colour: livery.
    // Same pitchOffset-tracking fix as `highlight` above, keyed to the
    // HOVERED workspace instead of the active one.
    Rectangle {
        id: previewRing
        visible: root.hoveredIndex >= 0
        z: 0
        width: 24
        height: 24
        radius: width / 2      // matches the active circle's curve, one size out
        color: "transparent"
        border.color: root.livery.paletteAccent
        border.width: 2
        opacity: 0.85
        anchors.verticalCenter: parent.verticalCenter
        anchors.verticalCenterOffset:
            root.hoveredIndex >= 0 ? root.pitchOffset(root.wsList[root.hoveredIndex]) : 0
        x: root.hoveredIndex >= 0
           ? root.hoveredIndex * (root.cellW + root.cellGap) + (root.cellW - width) / 2
           : 0
        Behavior on x {
            NumberAnimation { duration: 150; easing.type: Easing.OutCubic }
        }
        Behavior on anchors.verticalCenterOffset {
            NumberAnimation { duration: 150; easing.type: Easing.OutCubic }
        }
    }

    Row {
        id: cellRow
        z: 1
        spacing: root.cellGap
        anchors.verticalCenter: parent.verticalCenter

        Repeater {
            model: root.wsList
            delegate: Item {
                id: cell
                required property var modelData
                required property int index
                readonly property bool isActive: root.activeIndex === cell.index
                readonly property bool isUrgent: cell.modelData && cell.modelData.urgent
                // Previewed (a hovered terminal lives here) but NOT the active
                // ws — the note tints holoBlue to echo the ring. Active/urgent
                // both outrank the preview tint (they carry live meaning).
                readonly property bool isPreview: root.hoveredIndex === cell.index && !cell.isActive
                // Mouse-hover: previews this workspace as the selection target.
                readonly property bool isHovered: cellMouse.containsMouse

                width: root.cellW
                height: root.height

                // Hover-select pill — mousing a note previews it in the PRIMARY
                // gold (khoa): a soft gold underlay behind the glyph so you see
                // which workspace a click will jump to. Distinct from the active
                // note (its own hue + white outline). Declared before the glyph
                // so it renders behind it.
                Rectangle {
                    visible: cell.isHovered && !cell.isActive
                    anchors.centerIn: parent
                    // Same pitchOffset-tracking fix as `highlight`/`previewRing`
                    // above — this cell's own note sits at its own staff degree,
                    // not cell's plain centre.
                    anchors.verticalCenterOffset: root.pitchOffset(cell.modelData)
                    width: 22
                    height: 22
                    radius: width / 2   // same circle as the active/preview marks
                    color: root.livery.paletteAccent
                    opacity: 0.18
                }

                // The SOLID note glyph, parked at this note's pitch on the staff.
                // Resting → its own base16 accent hue (root.wsColor, one of 8
                // distinct colours cycled by workspace id — see LiveryState.
                // noteColor); the played note KEEPS its own hue but swells and
                // rests on a same-hue highlight pill; urgent → glitchPink. A
                // defining ink outline (paletteFg) rides only the active glyph.
                Text {
                    id: noteGlyph
                    anchors.horizontalCenter: parent.horizontalCenter
                    y: (cell.height - height) / 2 + root.pitchOffset(cell.modelData)
                    text: root.wsGlyph(cell.modelData)
                    color: cell.isActive ? root.wsColor(cell.modelData)
                          : (cell.isUrgent ? root.livery.glitchPink
                                           : (cell.isHovered ? root.livery.paletteAccent
                                                             : (cell.isPreview ? root.livery.paletteAccent
                                                                               : root.wsColor(cell.modelData))))
                    // Selected note = its OWN hue, swelled + pilled + given a
                    // WHITE outline (khoa) so it reads as highlighted against the
                    // opaque marble bar without changing its colour.
                    style: cell.isActive ? Text.Outline : Text.Normal
                    styleColor: Qt.rgba(1, 1, 1, 0.9)
                    font.family: "monospace"
                    font.pixelSize: cell.isActive ? root.activeSize : root.restSize
                    font.bold: true
                    Behavior on color { ColorAnimation { duration: 150 } }
                    Behavior on font.pixelSize {
                        NumberAnimation { duration: 140; easing.type: Easing.OutCubic }
                    }
                }

                // Urgent pulse (blink_red homage) — breathes the whole note.
                // alwaysRunToEnd: the last keyframe returns opacity to 1.0, so
                // when urgency clears mid-cycle the note settles fully opaque
                // instead of sticking dim.
                SequentialAnimation on opacity {
                    running: cell.isUrgent
                    loops: Animation.Infinite
                    alwaysRunToEnd: true
                    NumberAnimation { to: 0.35; duration: 600; easing.type: Easing.InOutQuad }
                    NumberAnimation { to: 1.0;  duration: 600; easing.type: Easing.InOutQuad }
                }

                MouseArea {
                    id: cellMouse
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: {
                        if (cell.modelData && cell.modelData.activate)
                            cell.modelData.activate()
                    }
                }
            }
        }
    }
    }

    // ── Note + bridge dependencies (injected by WidgetSlot) ────────────────
    required property var livery
    required property var bridge
    // The staging engine (StagingEngine singleton, injected as an extra —
    // it isn't part of WidgetSlot's universal livery/bridge contract) — the
    // calendar popout below asks it whether the active song (or, through
    // the baseline chain, sonata) dresses the "calendar" slot before
    // opening, and the popout's own embedded WidgetSlot needs the same
    // engine handle to resolve.
    required property var stagingEngine
    // The Exodos powermenu (AoideExodos instance, injected by shell.qml the
    // same way the launcher receives the clipboard). The clef toggles it
    // directly — the old `{ cmd: "powermenu" }` bridge line was a dead end
    // (shellbridge never parsed that command).
    required property var powermenu
    // The center-left dock (`dockSlot.item` — sonata's own `dock.qml`,
    // hosted by a `SurfaceSlot`), injected by shell.qml the SAME way —
    // audit-report.md flagged the ✎N cell's `{ cmd: "dock", action: "toggle" }`
    // as a silent no-op and suggested teaching shellbridge the command, but
    // that goes against this file's own already-stated rule two lines up:
    // ShellBridge is OUTBOUND-only by design (hazards.md §5, dock.qml's own
    // header — "no inbound CLI command to toggle a surface"), and adding one
    // here would be a second, inconsistent path to the exact toggle
    // GlobalShortcut (SUPER+G → aoide:dock) already owns cleanly. Fixed the
    // same way powermenu was: call .toggle() on the injected instance
    // directly.
    required property var dock
    // Shared session state (shell.qml's QtObject). Threaded through so the
    // centre WorkspaceRow can read `shared.hoveredWorkspace` — the gadget-dock
    // hover-preview bridge (concepts/Terminal-Commander). Optional/null-safe so
    // a standalone bar load never errors.
    property var shared: null

    // WidgetSlot (the host) sizes ITSELF off this item's implicitWidth/
    // Height, which is exactly backwards for a whole-width surface like the
    // bar: nothing anchors this Item to the window's width the way the old
    // declarative `AoideBar { anchors.fill: parent }` in shell.qml used to.
    // Bind width to the QML `parent` directly instead — under
    // `Component.createObject(root, props)` (WidgetSlot's mechanism), this
    // item's `parent` IS the WidgetSlot instance, and shell.qml anchors
    // THAT to fill the bar's PanelWindow — so this resolves to the real
    // window width once wired. `implicitHeight` stays this song's own
    // number (see the geometry note above); only width follows the host.
    width: parent ? parent.width : implicitWidth

    // The strip is 36px; the PanelWindow reserves exactly this — read back
    // dynamically now (shell.qml binds its exclusiveZone/implicitHeight off
    // this value), not pinned by the facet. Everything is painted within it
    // — the clef included — so no apron is needed (windows do not jump).
    readonly property int stripHeight: 36
    implicitHeight: stripHeight

    // ── Live "now" tick (1 s) — drives the hourly kaomoji rotation below ────
    property var now: new Date()
    Timer {
        interval: 1000
        repeat: true
        running: true
        onTriggered: root.now = new Date()
    }

    // ── Keep the default sink + source audio subobjects live ───────────────
    // Both nodes must be tracked or their `.audio` subobject never binds; the
    // source (microphone) gets the same treatment as the sink.
    PwObjectTracker {
        objects: {
            var o = []
            if (Pipewire.defaultAudioSink)   o.push(Pipewire.defaultAudioSink)
            if (Pipewire.defaultAudioSource) o.push(Pipewire.defaultAudioSource)
            return o
        }
    }

    // ── Volume helpers — OUTPUT / sink (Pipewire) ──────────────────────────
    readonly property var sinkAudio: (Pipewire.ready && Pipewire.defaultAudioSink
                                       && Pipewire.defaultAudioSink.audio)
                                      ? Pipewire.defaultAudioSink.audio : null
    readonly property bool volAvail: sinkAudio !== null
    readonly property bool volMuted: sinkAudio ? sinkAudio.muted : false
    readonly property int volPct: sinkAudio ? Math.round(sinkAudio.volume * 100) : 0
    function volIcon(pct) {
        if (pct <= 0) return "𝅗𝅥"
        if (pct < 34) return "♩"
        if (pct < 67) return "♪"
        if (pct < 90) return "♫"
        return "♬"
    }
    function volAdjust(deltaPct) {
        if (!sinkAudio) return
        var v = sinkAudio.volume + deltaPct / 100
        if (v < 0) v = 0
        if (v > 1) v = 1
        sinkAudio.volume = v
    }
    function volToggleMute() {
        if (sinkAudio) sinkAudio.muted = !sinkAudio.muted
    }

    // ── Microphone helpers — INPUT / source (Pipewire) ─────────────────────
    // Same shape as the sink: defaultAudioSource is a PwNode whose `.audio`
    // sub-object carries `volume` (capture gain, 0..1) and `muted`.
    readonly property var srcAudio: (Pipewire.ready && Pipewire.defaultAudioSource
                                      && Pipewire.defaultAudioSource.audio)
                                     ? Pipewire.defaultAudioSource.audio : null
    readonly property bool micAvail: srcAudio !== null
    readonly property bool micMuted: srcAudio ? srcAudio.muted : false
    readonly property int micPct: srcAudio ? Math.round(srcAudio.volume * 100) : 0
    function micAdjust(deltaPct) {
        if (!srcAudio) return
        var v = srcAudio.volume + deltaPct / 100
        if (v < 0) v = 0
        if (v > 1) v = 1
        srcAudio.volume = v
    }
    function micToggleMute() {
        if (srcAudio) srcAudio.muted = !srcAudio.muted
    }

    // ── Channel rosters — what the colonnade's pillar dropdowns list ───────
    // khoa, 2026-08-16. GROUNDED AGAINST quickshell-service-pipewire.qmltypes,
    // which settles the question the previous pass left open. `defaultAudioSink`
    // and `defaultAudioSource` are declared `isReadonly: true` — but the same
    // Component also declares:
    //
    //     Property { name: "preferredDefaultAudioSink"
    //                read:  "defaultConfiguredAudioSink"
    //                write: "setDefaultConfiguredAudioSink" … }
    //     Property { name: "preferredDefaultAudioSource"
    //                read:  "defaultConfiguredAudioSource"
    //                write: "setDefaultConfiguredAudioSource" … }
    //
    // with no `isReadonly`. So the default output and input ARE settable from
    // QML, and switching them needs no `pactl set-default-sink` and no
    // shell-out at all — unlike the bluez profile write one section down,
    // which has no native setter in either module and is the only thing here
    // that still leaves the process. Reads native, writes native: this seam
    // never touches a shell.
    //
    // The roster itself comes off `Pipewire.nodes`, which the same file gives
    // as an UntypedObjectModel of PwNode; each node carries `isSink`,
    // `isStream`, `nickname`/`description`/`name` and a `type` in
    // `PwNodeType::Flags`. Streams are applications, not devices, so they are
    // excluded; the `PwNodeType.Audio` bit excludes video nodes.
    property int pwEpoch: 0
    Connections {
        target: Pipewire.nodes
        function onObjectInsertedPost(obj, index) { root.pwEpoch++ }
        function onObjectRemovedPost(obj, index) { root.pwEpoch++ }
    }
    function nodeLabel(n) {
        var s = "" + (n.nickname || n.description || n.name || "")
        return s.length > 0 ? s : "node " + n.id
    }
    // The gloss beside the name: the node name's own tail, which is where
    // ALSA puts the port ("analog-stereo", "hdmi-stereo-extra1"). It answers
    // "which one is this" when two devices share a nickname.
    function nodeGloss(n) {
        var s = "" + (n.name || "")
        var dot = s.lastIndexOf(".")
        var tail = dot >= 0 ? s.substring(dot + 1) : s
        if (tail.length === 0) return ""
        return tail.length > 18 ? tail.substring(0, 17) + "…" : tail
    }
    function pwRoster(wantSink) {
        var epoch = root.pwEpoch          // the dependency that re-runs this
        var out = []
        if (!Pipewire.ready) return out
        var vals = (Pipewire.nodes && Pipewire.nodes.values)
                   ? Pipewire.nodes.values : []
        var cur = wantSink ? Pipewire.defaultAudioSink : Pipewire.defaultAudioSource
        for (var i = 0; i < vals.length; i++) {
            var n = vals[i]
            if (!n || n.isStream) continue
            if (!(n.type & PwNodeType.Audio)) continue
            if (n.isSink !== wantSink) continue
            out.push({ key: "" + n.id,
                       name: root.nodeLabel(n),
                       gloss: root.nodeGloss(n),
                       current: cur !== null && cur !== undefined && cur.id === n.id })
        }
        return out
    }
    readonly property var sinkRoster: pwRoster(true)
    readonly property var sourceRoster: pwRoster(false)
    function pwNodeById(key) {
        var vals = (Pipewire.nodes && Pipewire.nodes.values)
                   ? Pipewire.nodes.values : []
        for (var i = 0; i < vals.length; i++)
            if (vals[i] && ("" + vals[i].id) === ("" + key)) return vals[i]
        return null
    }
    function pickSink(key) {
        var n = root.pwNodeById(key)
        if (n) Pipewire.preferredDefaultAudioSink = n
    }
    function pickSource(key) {
        var n = root.pwNodeById(key)
        if (n) Pipewire.preferredDefaultAudioSource = n
    }

    // ── THE CHANNELS — per-application streams, the colonnade's naos payload ──
    // The nodes the three rosters above deliberately SKIP (`isStream`). The
    // `PwNodeType.Flags` enum carries AudioOutStream/AudioInStream directly, so
    // the direction is read off the type rather than guessed from isSink:
    // a playback stream feeds a sink and a capture stream drains a source, and
    // `isSink` means the opposite thing on each. (Grounded in
    // quickshell-service-pipewire.qmltypes: PwNodeType::Flag lists Untracked,
    // Audio, Video, Stream, Source, Sink, AudioSink, AudioSource, AudioDuplex,
    // AudioOutStream, AudioInStream, VideoSource, VideoSink.)
    //
    // Their `.audio` sub-object is the same seam the sink and source use, and
    // it has the same requirement: a node must be TRACKED or `.audio` never
    // binds (hazards.md §5). The tracker below carries the whole stream set,
    // so it grows and shrinks with the applications.
    readonly property var streamNodes: {
        var epoch = root.pwEpoch
        var out = []
        if (!Pipewire.ready) return out
        var vals = (Pipewire.nodes && Pipewire.nodes.values)
                   ? Pipewire.nodes.values : []
        for (var i = 0; i < vals.length; i++) {
            var n = vals[i]
            if (!n || !n.isStream) continue
            if (!(n.type & PwNodeType.Audio)) continue
            out.push(n)
        }
        return out
    }
    PwObjectTracker { objects: root.streamNodes }

    // The application's own name, then what it is playing. PipeWire stamps
    // `application.name` on every stream a client opens and `media.name` on
    // what is going through it, so the pair reads as "who · what" with no
    // shaping needed; the node's description is the fallback for a stream
    // that carries neither (some ALSA-compat clients).
    function streamLabel(n) {
        var p = n.properties || ({})
        var s = "" + (p["application.name"] || "")
        if (s.length === 0) s = "" + (n.description || "")
        if (s.length === 0) s = "" + (n.name || ("node " + n.id))
        return s
    }
    function streamGloss(n) {
        var p = n.properties || ({})
        var s = "" + (p["media.name"] || "")
        if (s.length === 0) s = "" + (p["media.role"] || "")
        if (s.length === 0) s = ""
        return s.length > 18 ? s.substring(0, 17) + "…" : s
    }
    // MONITORS ARE NOT CHANNELS, and the filter for them is `ready` — measured,
    // not assumed. Opening pavucontrol adds five `Stream/Input/Audio` nodes
    // named "PulseAudio Volume Control", one peak meter per bar it draws; they
    // outnumbered the three real applications in the naos five to three (live
    // capture, 2026-08-16). PipeWire does stamp `stream.monitor: true` on each
    // (confirmed in `pw-dump`), but Quickshell never surfaces it: with all
    // eight nodes handed to the tracker below, the three real streams report
    // `ready: true` and a full ~40-key `properties` map, while all five
    // monitors report `ready: false` and `properties: {}`, stably, still empty
    // 15s later (logged from this binding). So `stream.monitor` is not
    // readable here and the honest rule is the one that IS: a node whose
    // properties have not bound cannot be named, classified or explained, so
    // it does not get a row. The `stream.monitor` test stays underneath it as
    // the semantic filter, for the day such a node does bind.
    //
    // This also cannot move up into `streamNodes`: `.properties` does not bind
    // until a node is TRACKED (hazards.md §5) and the tracker is fed from that
    // list, so filtering there would ask a node a question it is not yet
    // answering. The tracker takes every audio stream; the roster is where
    // they drop out.
    readonly property var streamRoster: {
        var out = []
        var vals = root.streamNodes
        for (var i = 0; i < vals.length; i++) {
            var n = vals[i]
            if (!n.ready) continue
            var props = n.properties || ({})
            if (("" + props["stream.monitor"]) === "true") continue
            var a = n.audio
            out.push({ key: "" + n.id,
                       name: root.streamLabel(n),
                       gloss: root.streamGloss(n),
                       pct: a ? Math.round(a.volume * 100) : 0,
                       muted: a ? a.muted === true : false,
                       // AudioOutStream/AudioInStream are COMPOSITE flags, so
                       // the test is equality on the masked bits, not `!== 0`.
                       // Read off the live graph 2026-08-16: AudioOutStream is
                       // 21 (Audio 1 | Stream 4 | Sink 16), AudioInStream is 13
                       // (Audio 1 | Stream 4 | Source 8) — they share bits 1
                       // and 4, so `(13 & 21) !== 0` is 5 and a capture stream
                       // reads as playback. That is not theoretical: a live
                       // `parec` client filed itself under `playback` while the
                       // `recording` register said "nothing is recording"
                       // (captured, then fixed).
                       out: (n.type & PwNodeType.AudioOutStream)
                            === PwNodeType.AudioOutStream })
        }
        return out
    }
    function streamToggle(key) {
        var n = root.pwNodeById(key)
        if (n && n.audio) n.audio.muted = !n.audio.muted
    }
    function streamAdjust(key, deltaPct) {
        var n = root.pwNodeById(key)
        if (!n || !n.audio) return
        var v = n.audio.volume + deltaPct / 100
        if (v < 0) v = 0
        if (v > 1) v = 1
        n.audio.volume = v
    }

    // ── Bluetooth helpers (Quickshell.Bluetooth → bluez) ───────────────────
    // The third audio channel. `Bluetooth` is the bluez singleton; with bluez
    // down (or no adapter) `defaultAdapter` is null and every derived read
    // below degrades to the "no adapter" register — live-verified on
    // yomi-strix, where `hardware.bluetooth` is not enabled yet and
    // `systemctl is-active bluetooth` returns inactive.
    //
    // GROUNDED AGAINST THE COMPILED TYPES (quickshell-bluetooth.qmltypes,
    // Quickshell 0.3.0): BluetoothAdapter carries name/enabled/state/
    // discovering/devices; BluetoothDevice carries address/name/deviceName/
    // state/connected/paired/battery + connect()/disconnect()/pair()/forget().
    // There is NO profile property and NO profile method anywhere in that
    // module, and Quickshell.Services.Pipewire's own qmltypes expose only
    // nodes/links/linkGroups — no card and no profile object. So the A2DP ↔
    // headset switch CANNOT be driven natively; the write is the one place
    // this seam leaves QML (`Quickshell.execDetached`, the AoideClipboard
    // idiom), while the READ stays native off the PipeWire node's own
    // properties. Reads from services, side effects through a process — the
    // files-not-processes rule kept intact.
    readonly property var btAdapter: Bluetooth.defaultAdapter
    readonly property bool btAvail: btAdapter !== null
    readonly property bool btOn: btAvail && btAdapter.enabled

    // The connected device, hand-maintained. `Bluetooth.devices` only inserts
    // and removes on pairing changes — a device merely CONNECTING mutates the
    // existing object, so a plain `.values` binding would never re-fire (the
    // same class of problem the tray's `_recountTray()` solves below).
    property var btDev: null
    readonly property bool btConnected: btDev !== null
    readonly property string btName: btDev
        ? ("" + (btDev.name || btDev.deviceName || "device")) : ""
    function btShortName(n) {
        var s = "" + btName
        if (s.length <= n) return s
        return s.substring(0, n - 1) + "…"
    }
    // Bumped alongside the connected-device rescan so the ROSTER re-reads on
    // the same edges: the model is silent on a connect, so without this the
    // dropdown would show a device's old state until something else moved.
    property int btEpoch: 0
    function _rescanBt() {
        var vals = (Bluetooth.devices && Bluetooth.devices.values)
                   ? Bluetooth.devices.values : []
        var found = null
        for (var i = 0; i < vals.length; i++) {
            if (vals[i] && vals[i].connected) { found = vals[i]; break }
        }
        btDev = found
        btEpoch++
    }
    // The BT bay's roster. `Quickshell.Bluetooth`'s BluetoothDevice gives only
    // connect/disconnect/pair/cancelPair/forget (checked against
    // quickshell-bluetooth.qmltypes — see the grounding note above), so
    // selecting a row can only mean "connect this, or drop it if it is already
    // connected". Pairing is left to blueman; this is a switch, not a manager.
    readonly property var btRoster: {
        var epoch = root.btEpoch
        var out = []
        var vals = (Bluetooth.devices && Bluetooth.devices.values)
                   ? Bluetooth.devices.values : []
        for (var i = 0; i < vals.length; i++) {
            var d = vals[i]
            if (!d) continue
            out.push({ key: "" + d.address,
                       name: "" + (d.deviceName || d.name || d.address),
                       gloss: d.connected ? "connected"
                            : (d.paired ? "paired" : "seen"),
                       current: d.connected === true })
        }
        return out
    }
    function pickBtDevice(key) {
        var vals = (Bluetooth.devices && Bluetooth.devices.values)
                   ? Bluetooth.devices.values : []
        for (var i = 0; i < vals.length; i++) {
            var d = vals[i]
            if (!d || ("" + d.address) !== ("" + key)) continue
            if (d.connected) d.disconnect(); else d.connect()
            return
        }
    }
    Connections {
        target: Bluetooth
        function onDefaultAdapterChanged() { root._rescanBt() }
    }
    Connections {
        target: Bluetooth.devices
        function onObjectInsertedPost(obj, index) { root._rescanBt() }
        function onObjectRemovedPost(obj, index) { root._rescanBt() }
    }
    // Per-device connect/disconnect edge. Zero-size invisible watchers — the
    // model itself is silent on a connect, so each device is watched directly.
    Repeater {
        model: Bluetooth.devices
        delegate: Item {
            required property var modelData
            width: 0; height: 0; visible: false
            readonly property bool conn: modelData ? modelData.connected : false
            onConnChanged: root._rescanBt()
            Component.onCompleted: root._rescanBt()
        }
    }

    // Active profile, read NATIVELY off the PipeWire node PipeWire itself
    // publishes for the bluez card: module-bluez5 stamps `api.bluez5.profile`
    // onto every node it creates ("a2dp-sink" / "headset-head-unit"). The node
    // name is the fallback probe. Both nodes are already tracked by the
    // PwObjectTracker above, which is what makes `.properties` bind at all.
    function _btProfileOf(node) {
        if (!node) return ""
        var p = node.properties
        var v = p ? ("" + (p["api.bluez5.profile"] || "")) : ""
        if (v === "") v = "" + (node.name || "")
        v = v.toLowerCase()
        if (v.indexOf("a2dp") >= 0) return "a2dp"
        if (v.indexOf("headset") >= 0 || v.indexOf("hfp") >= 0
            || v.indexOf("hsp") >= 0) return "hsp"
        return ""
    }
    readonly property string btProfile: {
        if (!btConnected) return ""
        var a = _btProfileOf(Pipewire.defaultAudioSink)
        if (a !== "") return a
        return _btProfileOf(Pipewire.defaultAudioSource)
    }

    function btTogglePower() {
        if (btAdapter) btAdapter.enabled = !btAdapter.enabled
    }
    // The one shell-out in this seam (see the grounding note above). The card
    // name is PulseAudio/PipeWire's own stable convention: `bluez_card.` plus
    // the device address with ':' → '_'. UNVERIFIED on this rig — no adapter
    // is up here; verify with `pactl list cards short` once bluez runs.
    function btPickProfile(p) {
        if (!btDev || !btDev.address) return
        var card = "bluez_card." + ("" + btDev.address).replace(/:/g, "_")
        Quickshell.execDetached(["pactl", "set-card-profile", card,
                                 p === "hsp" ? "headset-head-unit" : "a2dp-sink"])
    }

    // The mixer button in the colonnade's ledger. Detached, argv form — never
    // a constructed shell string (AoideClipboard.copyById's rule).
    function openMixer() {
        Quickshell.execDetached(["pavucontrol"])
    }

    // ── Battery helpers (UPower) ───────────────────────────────────────────
    readonly property var battDev: UPower.displayDevice
    readonly property bool battAvail: battDev && battDev.isLaptopBattery && battDev.isPresent
    readonly property int battPct: battDev ? Math.round(battDev.percentage) : 0
    readonly property bool battCharging: battDev && battDev.state === UPowerDeviceState.Charging
    readonly property bool battFull: battDev && battDev.state === UPowerDeviceState.FullyCharged
    // Rest-notation icons by charge (𝄽 𝄾 𝄿 𝅀 𝅁 𝅂) — the battery drains toward
    // silence; full 𝆑, charging 𝄮.
    function battIcon() {
        if (battFull) return "𝆑"
        if (battCharging) return "𝄮"
        var rests = ["𝄽", "𝄾", "𝄿", "𝅀", "𝅁", "𝅂"]
        var idx = Math.floor(battPct / 100 * (rests.length - 1))
        if (idx < 0) idx = 0
        if (idx >= rests.length) idx = rests.length - 1
        return rests[idx]
    }
    function battBar(pct) {
        var cells = 10
        var p = pct
        if (p < 0) p = 0
        if (p > 100) p = 100
        var filled = Math.round(p / 100 * cells)
        var s = "["
        for (var i = 0; i < cells; i++) s += (i < filled) ? "▓" : "░"
        s += "]"
        return s
    }
    // Time-remaining string from UPower timeToEmpty / timeToFull (seconds).
    function battTime() {
        if (!battDev) return ""
        var sec = battCharging ? battDev.timeToFull : battDev.timeToEmpty
        if (!sec || sec <= 0) return ""
        var min = Math.round(sec / 60)
        var h = Math.floor(min / 60)
        var m = min % 60
        return (battCharging ? "full in " : "left ") +
               (h > 0 ? (h + "h" + (m < 10 ? "0" : "") + m) : (m + "m"))
    }
    readonly property bool battWarn: battAvail && !battCharging && battPct <= 20
    readonly property bool battCrit: battAvail && !battCharging && battPct <= 10
    // Blink toggle for warning/critical.
    property bool blinkOn: true
    Timer {
        interval: 1000
        repeat: true
        running: root.battWarn
        onTriggered: root.blinkOn = !root.blinkOn
    }

    // ── Agent-sessions count (Aoide-native — sessions.json via FileView) ────
    // Reuses the TerminalManagerGadget data seam (already-plumbed stage file).
    // Click → open the gadget dock (bridge dock command, the AoideAgentWidgets path).
    property int sessionCount: 0
    property bool sessionsBlocked: false
    property bool hooksBlocked: false
    // A human is summoned when ANY session's merged live state is blocked —
    // either a roster state (sessions.json) OR a live hook phase (hooks.json,
    // which overrides the roster in graph.rs::merged_sessions). Either source
    // flips the ✎N cell from paletteHot to the urgent role (glitchPink) + pulse.
    readonly property bool anyBlocked: sessionsBlocked || hooksBlocked
    function anyStateBlocked(arr, key) {
        if (!arr) return false
        for (var i = 0; i < arr.length; i++) {
            var v = arr[i] ? ("" + (arr[i][key] || "")).toLowerCase() : ""
            if (v.indexOf("block") !== -1) return true
        }
        return false
    }
    // CONDUCTING files (CONTRACTS.md §4) — state/stage/, not song/stage/.
    readonly property string sessionsPath:
        (Quickshell.env("AOIDE_ROOT") || (Quickshell.env("HOME") + "/.aoide")) + "/state/stage/sessions.json"
    readonly property string hooksPath:
        (Quickshell.env("AOIDE_ROOT") || (Quickshell.env("HOME") + "/.aoide")) + "/state/stage/hooks.json"
    FileView {
        id: sessionsFile
        path: root.sessionsPath
        watchChanges: true
        onFileChanged: sessionsFile.reload()
        onTextChanged: {
            try {
                var d = JSON.parse(sessionsFile.text())
                var arr = (d && d.sessions) ? d.sessions : []
                root.sessionCount = arr.length
                root.sessionsBlocked = root.anyStateBlocked(arr, "state")
            } catch (e) { /* absent/garbage → hold count */ }
        }
        Component.onCompleted: sessionsFile.reload()
    }
    FileView {
        id: hooksFile
        path: root.hooksPath
        watchChanges: true
        onFileChanged: hooksFile.reload()
        onTextChanged: {
            try {
                var d = JSON.parse(hooksFile.text())
                root.hooksBlocked = root.anyStateBlocked(d && d.hooks, "phase")
            } catch (e) { /* absent/garbage → hold */ }
        }
        Component.onCompleted: hooksFile.reload()
    }

    // ── Network (glyph: procfs FileView · stele: Quickshell.Networking) ─────
    // /proc/net/route lists every iface that has a route; the default route is
    // the row whose Destination field is "00000000". Reading a kernel file (not
    // spawning a process) keeps us inside the no-shell-out rule. Real iface
    // names — classified wifi vs ethernet by prefix (wl* → wifi).
    //
    // The GLYPH keeps this procfs read (cheap, service-independent, proven);
    // the DIKTYON popout below reads Quickshell.Networking instead — the
    // native NM binding this Quickshell rev ships (a service call in the
    // SystemTray/UPower class, sanctioned by hazards.md §5, not a shell-out).
    // The two disagree only in the window where NM knows something the route
    // table doesn't yet, which is exactly when the popout is the truth.
    property string netKind: "down"   // "wifi" | "eth" | "down"
    function parseRoute(text) {
        if (!text) return "down"
        var lines = ("" + text).split("\n")
        for (var i = 1; i < lines.length; i++) {
            var parts = lines[i].trim().split(/\s+/)
            if (parts.length < 2) continue
            if (parts[1] === "00000000") {
                var n = parts[0].toLowerCase()
                if (n.indexOf("wl") === 0 || n.indexOf("wlan") === 0) return "wifi"
                return "eth"
            }
        }
        return "down"
    }
    FileView {
        id: routeFile
        path: "/proc/net/route"
        onTextChanged: root.netKind = root.parseRoute(routeFile.text())
        Component.onCompleted: routeFile.reload()
    }
    Timer {
        interval: 5000
        repeat: true
        running: true
        onTriggered: routeFile.reload()
    }
    function netGlyph(kind) {
        if (kind === "wifi") return "𝆹𝅥𝅮"
        if (kind === "eth")  return "𝆺𝅥𝅯"
        return "𝄽"   // a rest — the line has gone silent
    }
    function netLabel(kind) {
        if (kind === "wifi") return "wifi"
        if (kind === "eth")  return "eth"
        return "off"
    }

    // DIKTYON popout state + native device handles. Same hand-scan idiom as
    // the Bluetooth block above (_rescanBt): rescan on model edges, plus a
    // zero-size watcher Repeater for the property changes the model itself
    // is silent on. First device of each type wins — this rig carries one
    // wifi radio and one NIC, and a second of either is a redesign, not a
    // loop tweak.
    property bool netOpen: false
    property var wifiDev: null
    property var wiredDev: null
    function _rescanNetDevices() {
        var vals = (Networking.devices && Networking.devices.values)
                   ? Networking.devices.values : []
        var w = null, e = null
        for (var i = 0; i < vals.length; i++) {
            var d = vals[i]
            if (!d) continue
            if (!w && d.type === DeviceType.Wifi)  w = d
            if (!e && d.type === DeviceType.Wired) e = d
        }
        wifiDev = w
        wiredDev = e
    }
    Connections {
        target: Networking.devices
        function onObjectInsertedPost(obj, index) { root._rescanNetDevices() }
        function onObjectRemovedPost(obj, index)  { root._rescanNetDevices() }
    }
    // Active scanning only while the stele is open — WifiDevice.scannerEnabled
    // asks NM for a live scan loop; leaving it on full-time burns radio time
    // for a closed popout. `when` guards the null window before NM reports.
    Binding {
        target: root.wifiDev
        property: "scannerEnabled"
        value: root.netOpen
        when: root.wifiDev !== null
    }

    // ── Rice mode vocabulary (Aoide-native — livery.riceMode) ───────────────
    // The rice engine's edit-state, one of exactly three strings (storage::
    // mode's RiceMode, lowercase on the wire, hot from stage/mode.json via
    // LiveryState): is this manuscript under the pen right now?
    //   declarative → || decl — the measure is CLOSED. Live writes refused;
    //     what's true is what the last home-manager switch baked. Plain
    //     ASCII double bar reads as "finished and published" without
    //     touching the font's Unicode fallback chain at all. Resting ink,
    //     dimmed — the safe default should recede, not glow. (NOT 𝄽: the
    //     idle rest already works two jobs on this strip — mute + net-down —
    //     a third would let "𝄽 decl" sit two cells from "𝄽 off". TWO glyphs
    //     were tried and rejected here after live testing this session: 𝄂
    //     U+1D102 renders as a bare "|" fallback, and ‖ U+2016 — despite
    //     being common General Punctuation, not a rare SMP symbol — STILL
    //     rendered wrong (a stray "/") on this font stack. Lesson sharpened
    //     past the trayToggle scar: it's not just rare SMP glyphs that need
    //     live verification before trusting them, ANY non-ASCII glyph does,
    //     on this stack. Plain "||" sidesteps the whole fallback chain.)
    //   staging     → ♪ stage — the measure is OPEN, a note under the pen:
    //     hot-load unlocked, rice stage/cover set write the live desktop.
    //     Gold — the bar's open/active register (open toggles, the active
    //     workspace) AND the state tier's own working→gold partnering
    //     (grammar §2): the engine is literally in its working state.
    //   draft       → 𝄋 draft — dal segno: stage writes route through a
    //     saved mark (a draft snapshot), never the committed song. Aegean
    //     holoBlue — the PREVIEW register (the workspace hover-ring's
    //     "a copy, not the real thing"), distinct from both resting ink
    //     and live gold. Not urgent: no mode is an alarm.
    // Unknown strings fall through to the declarative row — the same
    // safe-default reading LiveryState and mode.rs apply to an absent or
    // corrupt marker.
    function modeGlyph(m) {
        if (m === "staging") return "♪"
        if (m === "draft")   return "𝄋"
        return "||"
    }
    function modeWord(m) {
        if (m === "staging") return "stage"
        if (m === "draft")   return "draft"
        return "decl"
    }
    function modeColor(m) {
        if (m === "staging") return root.livery.paletteAccent
        if (m === "draft")   return root.livery.holoBlue
        return root.livery.paletteFg
    }

    // ── Window title (Hyprland active toplevel) with kaomoji empty-rewrite ──
    // A small songbook of music kaomoji combos; the empty-title rewrite picks
    // one at RANDOM, re-rolled each time the active window changes, so an empty
    // workspace hums a fresh little face instead of the same hourly one.
    readonly property var kaomojiSet: [
        "/ᐠ - ˕ -マ Ⳋ ⋆｡°✩♬ ♪",
        "♪(´▽｀) ⋆｡°✩",
        "•¨•.¸¸♪ ヾ(´〇`)ﾉ ♬",
        "(￣▽￣)/♫ •*¨*•.¸¸♪",
        "✧*。٩(ˊᗜˋ*)و ♪ ✧*。",
        "₊˚⊹ ♡ ♬ ⋆｡°✩"
    ]
    property int kaomojiIdx: 0
    function rollKaomoji() {
        root.kaomojiIdx = Math.floor(Math.random() * root.kaomojiSet.length)
    }
    Component.onCompleted: { rollKaomoji(); _recountTray(); _rescanNetDevices() }
    // Re-roll on every active-window change — so landing on an empty workspace
    // shows a freshly-random face (it's only displayed when the title is empty).
    Connections {
        target: Hyprland
        function onActiveToplevelChanged() { root.rollKaomoji() }
    }
    readonly property string kaomoji: kaomojiSet[kaomojiIdx]
    function winTitle() {
        var t = (Hyprland.activeToplevel && Hyprland.activeToplevel.title)
                ? ("" + Hyprland.activeToplevel.title) : ""
        if (t.length === 0) return kaomoji
        if (t.length > 25) return t.substring(0, 24) + "…"
        return t
    }

    // ── Popout visibility state ─────────────────────────────────────────────
    // khoa, 2026-08-15: hover no longer opens the colonnade. The audio cells
    // split into TWO surfaces, the way the manuscript splits a cue staff from
    // the part it glosses:
    //   · HOVER  → the CUE — a small self-framed stele reading out vol, mic
    //              and bluetooth. Informational, never interactive.
    //   · CLICK  → the COLONNADE — the full three-bay porch, click-LATCHED
    //              (`audioOpen`), the same toggle idiom the tray fermata and
    //              the calendar already use on this strip.
    // The cue stands down while the colonnade is open, so the two never stack.
    //
    // khoa, 2026-08-16: the cue RETAINS on its own body again. Yesterday's
    // build derived `audioCueShown` from the bar cell alone, which made the
    // card unreachable — leaving the cell to go to it was what closed it.
    // Three latches now feed one grace timer:
    //   · `volCellHover`/`micCellHover` — the bar cells (unchanged)
    //   · `cueBodyHover`  — set by the ONE MouseArea at the cue's root, which
    //                       is the card's only hit-testing authority for a
    //                       measured reason recorded on the component itself.
    //                       Its `anchors.topMargin: -4` reaches past the
    //                       stele's own bounds to claim StelePopout's 4px host
    //                       gap, the strip of popup between the cell and this
    //                       card that belongs to no item.
    //   · `cueHotRow`     — which channel row the pointer is on, as an INDEX and
    //                       not a bool: two rows racing on enter/exit order
    //                       can settle a shared bool false with the pointer
    //                       still inside one of them. An index only ever
    //                       clears itself (`if (cueHotRow === row)`), so the
    //                       order cannot corrupt it. It doubles as the
    //                       row-hover read, so no row needs its own.
    // Each latch has exactly ONE writer, which is what makes the union safe.
    // `cueGrace` (220ms) covers the frame where every latch is momentarily
    // false mid-handoff. Opening the colonnade drops the cue immediately
    // rather than through the grace, so the two never overlap even for 220ms.
    property bool volCellHover: false  // pointer over the vol cell
    property bool micCellHover: false  // pointer over the mic cell
    property bool cueBodyHover: false  // pointer inside the cue stele's body
    property int  cueHotRow: -1        // channel row under the pointer (-1 none)
    property bool audioOpen: false     // the colonnade — click-latched
    readonly property bool audioCellHover: volCellHover || micCellHover
    readonly property bool cueRetain: cueBodyHover || cueHotRow >= 0
    readonly property bool cueWanted: (audioCellHover || cueRetain) && !audioOpen
    property bool audioCueShown: false
    onCueWantedChanged: {
        if (cueWanted) { cueGrace.stop(); audioCueShown = true }
        else cueGrace.restart()
    }
    onAudioOpenChanged: {
        if (audioOpen) {
            cueGrace.stop()
            cueBodyHover = false
            cueHotRow = -1
            audioCueShown = false
        }
    }
    Timer {
        id: cueGrace
        interval: 220
        onTriggered: if (!root.cueWanted) root.audioCueShown = false
    }
    readonly property bool audioShown: audioCellHover || cueRetain || audioOpen
    property bool battShown: false     // battery hover popout
    property bool calShown: false      // calendar click popout (song widget slot)

    // ── System tray (StatusNotifier) — popup ───────────────────────────────
    // The fermata toggle in the right stave opens/closes the tray popout (the
    // BarPopout in the POPOUTS section below, hung under the toggle cell).
    // Starts closed; the toggle only appears once at least one item has
    // registered (no empty popout on a fresh session).
    property bool trayOpen: false
    // trayCount is a hand-maintained count, NOT a declarative binding on
    // SystemTray.items.values.length — confirmed live (console-probe on the
    // running desktop) that a plain `.values.length` binding DOES actually
    // stay reactive across insert/remove here, so that wasn't the bug this
    // was first written to fix. Kept anyway as the more robust idiom: model
    // list signals (onObjectInsertedPost/onObjectRemovedPost) are a firmer
    // contract than trusting a derived property's dependency capture on a
    // JS array snapshot, and it already matches SurfaceSlot._manifestWatch's
    // pattern elsewhere in this codebase for the same class of problem.
    // SystemTray.onItemRegistered/onItemUnregistered are plain METHODS in
    // the qmltypes, not signals — a Connections handler on them would
    // compile and silently never fire; don't reach for those instead.
    //
    // The actual bug (root-caused with pixel-diff screenshots, not a guess):
    // the toggle Text below had font.bold: true. This system's monospace
    // bold face has no glyph for U+1D110/U+1D111 (the fermata pair) and
    // Quickshell renders that as fully invisible — confirmed by forcing
    // `visible: true` unconditionally and still seeing zero pixel change,
    // then removing font.bold and immediately seeing the glyph render. The
    // toggle was reachable and correctly gated the whole time; it was
    // rendering nothing.
    property int trayCount: 0
    function _recountTray() {
        var vals = SystemTray.items ? SystemTray.items.values : null
        trayCount = vals ? vals.length : 0
        if (trayCount === 0) trayOpen = false   // no stuck-open empty popout
    }
    Connections {
        target: SystemTray.items
        function onObjectInsertedPost(obj, index) { root._recountTray() }
        function onObjectRemovedPost(obj, index) { root._recountTray() }
    }

    // ══ MUSICAL GEOMETRY ═══════════════════════════════════════════════════
    // The staff sits at the strip's vertical midline; five lines a staffGap
    // apart. Content is written on the staff, so every cell centres on it.
    readonly property real staffMid: stripHeight / 2
    readonly property real staffGap: 3.5
    readonly property real staffSpan: staffGap * 4   // top line → bottom line
    readonly property int edgePad: 8

    // ── Inline notation vocabulary ─────────────────────────────────────────

    // A barline drawn across the staff — a black engraved section divider (┃/┼).
    component Barline: Rectangle {
        width: 1.5
        height: root.staffSpan + 4
        radius: 0.5
        color: root.livery.barFg
        opacity: 0.7
        anchors.verticalCenter: parent.verticalCenter
    }

    // ══ THE MANUSCRIPT STRIP ═══════════════════════════════════════════════
    // A single OPAQUE sheet of manuscript paper — flat, fully solid
    // paletteBg, no glass, no gloss. Rounded ends give the ╭─ … ─╮ read of
    // the sketch.
    Rectangle {
        id: page
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        height: root.stripHeight
        radius: 0                        // EDGED — hard square corners, no round
        // Fully opaque paletteBg — no glass, no translucency. The bar reads
        // as a flat solid strip; the black staff ink sits directly on the
        // song's page colour with no blur or gradient behind it.
        color: Qt.rgba(Qt.color(root.livery.paletteBg).r,
                       Qt.color(root.livery.paletteBg).g,
                       Qt.color(root.livery.paletteBg).b, 1.0)
        opacity: 1.0
    }
    // The page rail — a DEFINED black edge framing the opaque strip.
    Rectangle {
        anchors.fill: page
        radius: 0
        color: "transparent"
        border.color: root.livery.barFg
        border.width: 1
        opacity: 0.5
    }

    // ── THE STAFF — five lines ruled the full width, at the strip midline.
    // They pass behind every cell; the note-heads (WorkspaceRow) land on them.
    Repeater {
        model: 5
        Rectangle {
            required property int index
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.leftMargin: root.edgePad + 26   // clear of the clef
            anchors.rightMargin: root.edgePad + 10
            height: 1
            y: root.staffMid + (index - 2) * root.staffGap
            color: root.livery.barFg
            opacity: 0.55
        }
    }

    // ══ HEAD — the treble clef, the key of the piece AND the powermenu key ══
    Text {
        id: clefText
        anchors.left: parent.left
        anchors.leftMargin: root.edgePad
        anchors.verticalCenter: parent.verticalCenter
        text: "𝄞"
        color: root.livery.paletteFg
        font.family: "monospace"
        // The treble clef glyph is TALL (big loop + descender tail); at 20px it
        // clipped against the 36px strip's top/bottom. 16px + vertical-fit keeps
        // the whole clef inside the bar without an apron/overhang.
        font.pixelSize: 16
        verticalAlignment: Text.AlignVCenter
        fontSizeMode: Text.VerticalFit
        height: root.stripHeight
        font.bold: true
        MouseArea {
            anchors.fill: parent
            cursorShape: Qt.PointingHandCursor
            // Direct toggle on the injected AoideExodos instance — the same
            // sibling-reference idiom as launcher↔clipboard. (The old
            // `sendCommand({ cmd: "powermenu" })` went nowhere: shellbridge's
            // parser never knew that command.)
            onClicked: if (root.powermenu) root.powermenu.toggle()
        }
    }

    // ══ LEFT STAVE — sessions · gadget tray · clock / title ════════════════
    // Written just after the clef, reading left to right like the opening of
    // the measure. Everything centres on the staff.
    Row {
        id: leftContent
        anchors.left: clefText.right
        anchors.leftMargin: 16   // clear of the staff-line start (edgePad+26); 12 sat the pencil right on top of it
        anchors.verticalCenter: parent.verticalCenter
        spacing: 9

        // Aoide-native: live agent-sessions cell → click opens the dock.
        Text {
            id: sessionsCell
            anchors.verticalCenter: parent.verticalCenter
            visible: root.sessionCount > 0
            text: "✎" + root.sessionCount
            // Black ink at rest; when ANY session is blocked it switches to the
            // urgent role (glitchPink) and pulses — a summons from across the bar.
            color: root.anyBlocked ? root.livery.glitchPink : root.livery.paletteFg
            font.family: "monospace"
            font.pixelSize: 13
            font.bold: true

            SequentialAnimation on opacity {
                running: root.anyBlocked
                loops: Animation.Infinite
                NumberAnimation { to: 0.35; duration: 600; easing.type: Easing.InOutQuad }
                NumberAnimation { to: 1.0;  duration: 600; easing.type: Easing.InOutQuad }
            }

            MouseArea {
                anchors.fill: parent
                cursorShape: Qt.PointingHandCursor
                onClicked: if (root.dock) root.dock.toggle()
            }
        }

        // ── Clock + date. Lives on the bar, not the dock — the ambient face
        // belongs beside the measure, not in the case.
        Text {
            id: clockText
            anchors.verticalCenter: parent.verticalCenter
            text: Qt.formatDateTime(root.now, "hh:mm AP  ddd MMM dd")
            color: root.calShown ? root.livery.paletteAccent : root.livery.paletteFg
            font.family: "monospace"
            font.pixelSize: 14
            font.bold: true

            // Click opens the calendar popout — a per-song flavor-widget
            // slot (WidgetSlot below). Nothing to click through to when the
            // active song hasn't authored one; the popout itself gates on
            // stagingEngine.has(...) so an unauthored calendar simply won't open.
            MouseArea {
                anchors.fill: parent
                cursorShape: Qt.PointingHandCursor
                onClicked: root.calShown = !root.calShown
            }
        }
        // The " / " separator — a slur between clock and title.
        Text {
            id: slashText
            anchors.verticalCenter: parent.verticalCenter
            text: "/"
            color: root.livery.paletteFg
            font.family: "monospace"
            font.pixelSize: 14
            opacity: 0.55
        }
        // Active-window title (music kaomoji when empty). khoa, 2026-07-31:
        // primary-color ink — paletteAccent, not paletteFg.
        // Width capped to whatever room is left before the centre stave, so
        // elide actually engages — a Row gives a Text its implicitWidth
        // otherwise, and ElideRight never fires. Bound in terms of the
        // stave's own x plus the fixed-width neighbors and the Row's own
        // spacing (never the title's own x — that's Row-managed, a binding
        // loop waiting to happen).
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: root.winTitle()
            color: root.livery.paletteAccent
            font.family: "monospace"
            font.pixelSize: 14
            font.bold: true
            elide: Text.ElideRight
            width: Math.min(implicitWidth, Math.max(0,
                centreStave.x - leftContent.x - sessionsCell.width -
                slashText.width - clockText.width -
                3 * leftContent.spacing - 14))
        }
    }

    // ══ CENTRE — the melody: workspaces as note-heads, flanked by barlines ══
    // Held at true screen-centre (khoa's keep), so the bar's motif sits on the
    // wallpaper's axis. WorkspaceRow draws the note-heads + the playhead.
    Row {
        id: centreStave
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.verticalCenter: parent.verticalCenter
        spacing: 8

        Barline {}
        WorkspaceRow {
            id: centeredWorkspaces
            anchors.verticalCenter: parent.verticalCenter
            livery: root.livery
            shared: root.shared
        }
        Barline {}
    }

    // ══ RIGHT STAVE — the expression marks, closed by a final barline ══════
    Row {
        id: rightContent
        anchors.right: parent.right
        anchors.rightMargin: root.edgePad
        anchors.verticalCenter: parent.verticalCenter
        spacing: 10

        // Rice mode — Aoide-native: the manuscript's own edit-state, read
        // live off livery.riceMode (stage/mode.json). LEADS the right stave,
        // apart from the hardware expression marks (vol/mic/batt/net) that
        // follow — song-state before instrument-state, mirroring how the ✎N
        // Aoide cell leads the left stave. It also keeps the 𝄂 glyph far
        // from the DRAWN final barline closing this Row, so the strip never
        // shows two adjacent closing marks. A CONTROL, not just a status mark
        // (khoa, 2026-08-15): click is a two-way toggle — staging locks to
        // declarative; declarative OR draft unlocks back to staging — sent
        // through root.bridge.toggleRiceMode(), never a direct write from
        // QML. Glyph + word + colour per mode: root.modeGlyph's comment
        // carries the mapping.
        // NO font.bold — 𝄂 (U+1D102) is an SMP musical symbol this file has
        // only ever DRAWN (as rules), never rendered as text, and the bold
        // monospace face silently drops rare SMP glyphs (the fermata scar on
        // trayToggle below); regular weight is the verified-safe rendering.
        // If 𝄂 fails visual check even regular, fall back to 𝄽.
        Text {
            id: modeText
            anchors.verticalCenter: parent.verticalCenter
            text: root.modeGlyph(root.livery.riceMode) + " " +
                  root.modeWord(root.livery.riceMode)
            color: root.modeColor(root.livery.riceMode)
            // Declarative — locked, at rest, the ~always state — recedes
            // like the net cell's dead-link register; both unlocked modes
            // read at full strength (they're the news).
            opacity: root.livery.riceMode === "declarative" ? 0.55 : 1.0
            font.family: "monospace"
            font.pixelSize: 14

            MouseArea {
                anchors.fill: parent
                cursorShape: Qt.PointingHandCursor
                onClicked: root.bridge.toggleRiceMode()
            }
        }

        // Volume (OUTPUT) — scroll = adjust, click = open the colonnade,
        // hover = the cue readout. Attic-gold ink; hover feedback is opacity
        // + underline (the base is already the accent).
        //
        // khoa, 2026-08-16 — MIDDLE-CLICK WITHDRAWN. The 2026-08-15 build put
        // mute on the middle button here because left-click had just become
        // "open" and mute could not be allowed to cost a surface plus two
        // clicks. That premise is gone: the cue now stays open when entered
        // and every channel row on it is a one-click mute, so the cost is a
        // hover the pointer is already making plus one click — and it covers
        // bluetooth too, which middle-click never did. What was left was a
        // gesture with no mark on the cell, advertised only by a ledger line
        // on a DIFFERENT widget; both are gone. The colonnade's own per-column
        // click still mutes, unchanged.
        Text {
            id: volText
            anchors.verticalCenter: parent.verticalCenter
            visible: root.volAvail
            text: root.volMuted ? "𝄽 vol" : (root.volIcon(root.volPct) + " " + root.volPct)
            color: root.livery.paletteAccent
            opacity: root.audioShown ? 1.0 : 0.8
            font.underline: root.audioShown
            font.family: "monospace"
            font.pixelSize: 14
            font.bold: true
            MouseArea {
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onEntered: root.volCellHover = true
                onExited: root.volCellHover = false
                onClicked: root.audioOpen = !root.audioOpen
                onWheel: function(wheel) {
                    // gate on the vertical axis actually carrying a delta — a
                    // horizontal-only wheel event (trackpad swipe, a mouse's
                    // tilt-wheel) has angleDelta.y === 0, and `0 > 0 ? 2 : -2`
                    // silently fell to -2, i.e. ANY horizontal scroll read as
                    // "turn it down". Still swallows the gesture either way
                    // (accepted stays unconditional) — this only stops the
                    // wrong axis from moving the value.
                    if (wheel.angleDelta.y !== 0)
                        root.volAdjust(wheel.angleDelta.y > 0 ? 2 : -2)
                    wheel.accepted = true
                }
            }
        }

        // Microphone (INPUT) — the cool aegean channel, paired beside the vol cell.
        // Same gesture set as the vol cell: scroll = capture gain, click = open
        // the colonnade, hover = the cue readout (mute lives on the cue's own
        // `in` row as of 2026-08-16 — see the vol cell's note above).
        // ● recording dot when live; a 𝄽 rest when muted (mirrors the vol cell).
        Text {
            id: micText
            anchors.verticalCenter: parent.verticalCenter
            visible: root.micAvail
            text: root.micMuted ? "𝄽 mic" : ("● " + root.micPct)
            color: root.livery.holoBlue
            opacity: root.audioShown ? 1.0 : 0.8
            font.underline: root.audioShown
            font.family: "monospace"
            font.pixelSize: 14
            font.bold: true
            MouseArea {
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onEntered: root.micCellHover = true
                onExited: root.micCellHover = false
                onClicked: root.audioOpen = !root.audioOpen
                onWheel: function(wheel) {
                    // same angleDelta.y gate as the vol cell above — see its
                    // comment for why.
                    if (wheel.angleDelta.y !== 0)
                        root.micAdjust(wheel.angleDelta.y > 0 ? 2 : -2)
                    wheel.accepted = true
                }
            }
        }

        // Battery — a rest that deepens as charge drains; hover = time + bar.
        Text {
            id: battText
            anchors.verticalCenter: parent.verticalCenter
            visible: root.battAvail
            text: root.battFull
                  ? (root.battIcon() + " full")
                  : (root.battIcon() + " " + root.battPct + (root.battCharging ? "+" : ""))
            color: (root.battCrit || root.battWarn) ? root.livery.glitchPink
                                                    : root.livery.paletteFg
            opacity: (root.battWarn && !root.blinkOn) ? 0.3 : 1.0
            Behavior on opacity { NumberAnimation { duration: 400 } }
            font.family: "monospace"
            font.pixelSize: 14
            font.bold: true
            MouseArea {
                anchors.fill: parent
                hoverEnabled: true
                onEntered: root.battShown = true
                onExited: root.battShown = false
            }
        }

        // Network — black ink; a live link is full-strength, a dead link falls
        // to a dim rest (𝄽) at reduced opacity. Click toggles the DIKTYON
        // stele (StelePopout in the POPOUTS section) — an open cell takes the
        // accent + underline, the volume/tray open convention. Before
        // 2026-08-23 this cell was a bare Text with NO MouseArea at all —
        // "clicking it does nothing" was literal, not a wiring bug.
        Text {
            id: netText
            anchors.verticalCenter: parent.verticalCenter
            text: root.netGlyph(root.netKind) + " " + root.netLabel(root.netKind)
            color: root.netOpen ? root.livery.paletteAccent : root.livery.paletteFg
            opacity: root.netOpen ? 1.0 : (root.netKind === "down" ? 0.5 : 1.0)
            font.family: "monospace"
            font.pixelSize: 14
            font.underline: root.netOpen
            MouseArea {
                anchors.fill: parent
                cursorShape: Qt.PointingHandCursor
                onClicked: root.netOpen = !root.netOpen
            }
        }

        // ── System tray — the fermata toggle ───────────────────────────────
        // 𝄐 (U+1D110, fermata — the HOLD mark) at rest; 𝄑 (U+1D111,
        // fermata below) while open, the hold turned toward the popout
        // hanging under it. A fermata holds a note, and the popout's token
        // is `tray.held` — glyph and token say the same word. (The state
        // tier reads 𝄐 as "awaiting" (grammar §2), but the bar already
        // lets one glyph serve two registers — 𝄽 is the tier's idle AND
        // the mute/net-down rest on this strip — so the hold reading
        // stands here.) Click toggles the tray POPOUT (BarPopout below,
        // hung under this cell) — the held SNI icons live in the popup,
        // not on the staff. Resting ink like its neighbours; an open
        // toggle takes the accent, the volume-cell open/hover convention.
        // NO font.bold — this exact pair's own scar: U+1D110/U+1D111 are
        // rare SMP musical symbols this stack's bold monospace face
        // renders as fully INVISIBLE (pixel-diff-confirmed; the full
        // account lives on the trayCount comment above). Regular weight
        // is the verified-safe rendering — do not re-add bold here.
        Text {
            id: trayToggle
            anchors.verticalCenter: parent.verticalCenter
            visible: root.trayCount > 0
            text: root.trayOpen ? "𝄑" : "𝄐"
            color: root.trayOpen ? root.livery.paletteAccent : root.livery.paletteFg
            font.family: "monospace"
            font.pixelSize: 14
            MouseArea {
                anchors.fill: parent
                cursorShape: Qt.PointingHandCursor
                onClicked: root.trayOpen = !root.trayOpen
            }
        }

        // The final barline — thin + thick black rules, closing the measure (𝄂).
        Row {
            anchors.verticalCenter: parent.verticalCenter
            spacing: 2
            Rectangle {
                width: 1.5; height: root.staffSpan + 4; radius: 0.5
                color: root.livery.barFg; opacity: 0.7
                anchors.verticalCenter: parent.verticalCenter
            }
            Rectangle {
                width: 3; height: root.staffSpan + 4; radius: 0.5
                color: root.livery.barFg; opacity: 0.9
                anchors.verticalCenter: parent.verticalCenter
            }
        }
    }

    // ══ THE CUE — the audio hover readout ══════════════════════════════════
    // khoa, 2026-08-15: hovering an audio cell must NOT open the widget; it
    // shows current output volume, mic status and bluetooth status, and stops
    // there. In manuscript terms that is a CUE STAFF — the small stave a
    // copyist writes above a part so the player can see what the other channels
    // are doing without reading their parts.
    //
    // khoa, 2026-08-16: it is no longer read-only. A copyist's cue staff also
    // carries the TACET marks — where a channel falls silent — and that is the
    // one command this card grows: clicking a channel row silences it (mute for
    // out/in, adapter power for bt). Nothing else on it is clickable, and the
    // full control surface is still the colonnade one click away. It also
    // RETAINS on its own body now (see the popout-state block above); the
    // 2026-08-15 build could be looked at but never entered.
    //
    //   · ORDER   — the eleven parts (making-a-widget.md §1) at their floor
    //               sizes. A glance card is still a stele; there is no
    //               "tooltip" family in this house and this does not invent
    //               one. Width 217 (206 + the margin-mark column added
    //               2026-08-16), three body rows, no porch.
    //   · SIGNATURE — MUREX `livery.violet` (base0E), DELIBERATELY the same
    //               signature the colonnade wears: these are the same widget's
    //               two faces (glance and control), and a second hue would read
    //               as a second temple. It moved WITH the colonnade on
    //               2026-08-16 (base09 amber → murex, so the audio family stops
    //               sharing the calendar's clay) — the full derivation lives in
    //               AudioColonnade.qml's SIGNATURE note; the rule here is only
    //               that the two faces move together, always.
    //   · CROWN   — ♪ a single note, the small cue-note; the colonnade's ♫ is
    //               the beamed pair (two channels), so the crowns say which is
    //               the glance and which is the full score.
    //   · FRIEZE  — a CUE-STAFF course: three ruled hairlines at the same 3px
    //               pitch as the bar's own staff, stepped down the ladder
    //               (0.8 / 0.5 / 0.3). The bar it hangs off, shrunk.
    //   · LIVE FIGURE — the closing frame names the actual sink being
    //               controlled (`Pipewire.defaultAudioSink.nickname`), which
    //               appears nowhere else on this desktop.
    //
    // Every glyph here was charset-checked against the exact declared face
    // before use (hazards.md §1): ♪ ♩ ♫ ♬ 𝄽 𝄂 are Noto Music; ● and the
    // bluetooth mark U+F293 are JetBrainsMono Nerd Font. The generic
    // "monospace" alias the bar's own cells use — the face behind all three
    // recorded glyph failures — is not used anywhere in this component.
    // ── ONE CUE ROW — glyph · name · gauge or chip · value ─────────────────
    // A SIBLING of AudioCue, not a member of it: Quickshell's QML engine
    // rejects a `component` declared inside another `component` outright —
    // "Nested inline components are not supported", hit live on this file at
    // the first load of this build. Helper components nest inside an OBJECT
    // (launcher.qml's GlassPage lives inside its `book` Item) but never
    // inside another inline component.
    component CueChannel: Item {
        id: channel
        property var livery
        property string glyph: ""
        property string glyphFace: "Noto Music"
        property string name: ""
        property color hue: channel.livery ? channel.livery.paletteFg : "#000000"
        property bool live: true          // false → the whole row recedes
        property bool meter: false
        property int pct: 0
        property string chip: ""          // shown instead of a meter
        property string value: ""

        // khoa, 2026-08-16 — the row is a CONTROL now. It carries no MouseArea
        // of its own: the cue's single root MouseArea resolves which row the
        // pointer is in and calls `picked()` (the note there records why).
        // `row` is this channel's index in that scheme; `topInCue` is the top
        // edge it publishes for the hit test, derived from its own position so
        // the drawing and the hit band cannot drift apart. `hushed` is the
        // silenced state; `actionable` gates both click and hover for a channel
        // with nothing behind it.
        property int row: -1
        property bool actionable: false
        property bool hushed: false
        signal picked()

        readonly property real topInCue: channel.y + (channel.parent ? channel.parent.y : 0)
        readonly property bool hot: root.cueHotRow === channel.row && channel.actionable
        readonly property color ink: channel.livery ? channel.livery.paletteFg : "#000000"
        readonly property color urgent: channel.livery ? channel.livery.paletteUrgent : "#000000"
        readonly property real rowAlpha: live ? 1.0 : 0.4
        function withA(cstr, a) {
            var c = Qt.color(cstr)
            return Qt.rgba(c.r, c.g, c.b, a)
        }

        width: parent ? parent.width : 0
        height: 16

        // The hover ground — a faint wash of the row's own hue, so hover only
        // PROMOTES what is already drawn (making-a-widget.md §4). Declared
        // first so it sits behind the row's type.
        Rectangle {
            anchors.fill: parent
            anchors.leftMargin: -3; anchors.rightMargin: -3
            radius: 0
            color: channel.hot ? channel.withA(channel.hue, 0.13) : "transparent"
            Behavior on color { ColorAnimation { duration: 150 } }
        }
        // The hush mark — a 1px rule struck through a silenced row, in
        // terracotta, so the state reads from the SHAPE with the colour
        // removed (the colonnade encodes the same state as a broken column).
        Rectangle {
            anchors.left: parent.left; anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            anchors.verticalCenterOffset: 1
            height: 1
            visible: channel.hushed
            color: channel.withA(channel.urgent, 0.75)
        }

        // THE MARGIN MARK — the row's own "this line is a target". Added
        // 2026-08-16: at rest the only thing announcing that these rows were
        // pressable was the ledger's hint line and a cursor change, and a card
        // that gained a second gesture (the wheel) needs its affordances more,
        // not less. It is the manuscript row's margin column verbatim, the one
        // the colonnade's picker and naos rows already carry — which also
        // visually binds the cue's rows to theirs, the two faces sharing one
        // hand. Only an ACTIONABLE row gets one: a dead row is not merely
        // inert, it is untargetable, and it should not advertise otherwise.
        Text {
            id: vMark
            anchors.left: parent.left
            anchors.verticalCenter: parent.verticalCenter
            anchors.verticalCenterOffset: -1
            width: 11
            horizontalAlignment: Text.AlignHCenter
            text: channel.actionable ? "·" : ""
            font.family: "Noto Music"
            font.pixelSize: channel.hot ? 13 : 11
            color: channel.withA(channel.hue, channel.hot ? 1.0 : 0.3)
            Behavior on color { ColorAnimation { duration: 150 } }
        }
        Text {
            id: vGlyph
            anchors.left: vMark.right
            anchors.verticalCenter: parent.verticalCenter
            width: 15
            horizontalAlignment: Text.AlignHCenter
            text: channel.glyph
            font.family: channel.glyphFace
            font.pixelSize: 13
            color: channel.hue
            opacity: channel.rowAlpha
        }
        Text {
            id: vName
            anchors.left: vGlyph.right; anchors.leftMargin: 5
            anchors.verticalCenter: parent.verticalCenter
            text: channel.name
            font.family: "Noto Serif"; font.pixelSize: 11
            font.letterSpacing: 1
            color: channel.withA(channel.ink, (channel.hot ? 1.0 : 0.85) * channel.rowAlpha)
        }
        // the gauge — one hairline track, one fill, no box (the opacity
        // ladder does the separating; making-a-widget.md §3)
        Item {
            id: vGauge
            anchors.left: vName.right; anchors.leftMargin: 8
            anchors.verticalCenter: parent.verticalCenter
            width: 58; height: 5
            visible: channel.meter
            Rectangle {
                anchors.fill: parent; radius: 0
                color: channel.withA(channel.ink, 0.09)
                border.width: 1
                border.color: channel.withA(channel.ink, 0.35 * channel.rowAlpha)
            }
            Rectangle {
                anchors.left: parent.left; anchors.top: parent.top
                anchors.bottom: parent.bottom; anchors.margins: 1
                width: (parent.width - 2) * Math.max(0, Math.min(1, channel.pct / 100))
                radius: 0
                color: channel.withA(channel.hue, 0.9 * channel.rowAlpha)
                Behavior on width { NumberAnimation { duration: 140; easing.type: Easing.OutCubic } }
            }
        }
        Text {                            // the chip — the bt row's profile
            anchors.left: vName.right; anchors.leftMargin: 8
            anchors.verticalCenter: parent.verticalCenter
            visible: !channel.meter && channel.chip.length > 0
            text: channel.chip
            font.family: "JetBrainsMono Nerd Font"; font.pixelSize: 8
            font.letterSpacing: 1
            color: channel.withA(channel.ink, 0.55)
        }
        Text {
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            text: channel.value
            font.family: "JetBrainsMono Nerd Font"; font.pixelSize: 11
            color: channel.hushed ? channel.urgent
                                : channel.withA(channel.hue, channel.rowAlpha)
        }
    }

    component AudioCue: Item {
        id: cue

        required property var livery

        readonly property string faceSerif: "Noto Serif"
        readonly property string faceMono:  "JetBrainsMono Nerd Font"
        readonly property string faceMusic: "Noto Music"
        readonly property color sig: cue.livery.violet
        readonly property color ink: cue.livery.paletteFg

        function withA(cstr, a) {
            var c = Qt.color(cstr)
            return Qt.rgba(c.r, c.g, c.b, a)
        }
        function kaomojiFor() {
            // audit-B: this disagreed with AudioColonnade.qml's own
            // kaomojiFor() on mic-only-muted (volMuted false, micMuted true)
            // — that state fell through the old `if (root.volMuted)` branch
            // entirely and read as whatever came next (bt-connected/loud/
            // attentive), never as "one silenced". AudioColonnade's version
            // is the one that reads correct: it tallies BOTH channels
            // (mutedCount) and treats either single mute the same way. Made
            // to match — same glyph, either channel — rather than inventing
            // a third version.
            if (root.volMuted && root.micMuted) return "(-_- )"
            if (root.volMuted || root.micMuted) return "( ･_･)"
            if (root.btConnected) return "♪( ˘ω˘ )"
            if (root.volPct >= 85) return "♪(´▽｀)"
            return "( ･ω･)ﾉ"
        }
        // The three channel rows, in the order they are drawn — the index the
        // hit test speaks in, and the index each row carries as `row`.
        function channels() { return [channelOut, channelIn, channelBt] }
        // Which channel row a pointer Y (in this root's coordinates) falls in,
        // or -1. Skips a channel with nothing behind it, so a dead row is not
        // merely inert but untargetable.
        function rowAt(y) {
            var vs = cue.channels()
            for (var i = 0; i < vs.length; i++) {
                var v = vs[i]
                if (!v.actionable) continue
                if (y >= v.topInCue && y < v.topInCue + v.height) return i
            }
            return -1
        }
        function sinkName() {
            var n = Pipewire.defaultAudioSink
            var s = n ? ("" + (n.nickname || n.description || n.name || "")) : ""
            if (s.length === 0) return "no sink"
            return s.length > 16 ? s.substring(0, 15) + "…" : s
        }

        width: 217                            // 206 + the 11px margin-mark column
        implicitHeight: stele.height + 5      // +5 clears the cast shadow

        // ── THE RETENTION LATCHES · ONE HIT-TESTING AUTHORITY ───────────────
        // Everything the pointer does to this card goes through this single
        // MouseArea: it holds the card open, it claims StelePopout's host gap,
        // it decides which channel row is hot, and it dispatches the click. That
        // is not tidiness, it is what the measurements on 2026-08-16 left
        // standing. Arrangements were built and read off screen through a live
        // state print in this ledger:
        //   1. a filling MouseArea here PLUS a MouseArea per row — the root
        //      took every hover: `cueHotRow` read -1 at every pointer position
        //      down the card, and switching this one object's `hoverEnabled`
        //      off (nothing else changed) made the rows light immediately.
        //      Whatever accepts hover at this root gets it before a MouseArea
        //      nested inside the stele does.
        //   2. a `HoverHandler` here instead of the MouseArea, on the
        //      expectation that a handler composes with descendant MouseAreas
        //      rather than competing with them — the rows stayed dark, so it
        //      does not help.
        //   3. a 4px MouseArea anchored `bottom: parent.top` to claim the host
        //      gap — it latched true on entry and never received its exit, and
        //      read hovered at every depth down the card. An item lying wholly
        //      outside its parent's bounds gets its enter and not its leave.
        // So: one object, no nesting, no handler. The row band is resolved
        // ARITHMETICALLY from the pointer position, which needs no event
        // delivery to a nested item at all. Each channel publishes its own top
        // edge (`topInCue`) rather than the numbers being restated here, so
        // moving a band in the Column cannot desynchronise the hit test from
        // the drawing.
        //
        // WHAT WAS NOT VERIFIED BY POINTING AT IT AT THE TIME, and why: this
        // measurement predates `aoide screen point move` (wlrctl virtual-
        // pointer motion, not a warp — it DOES fire a motion event inside a
        // surface the pointer already entered, unlike a warp). At the time
        // this was written, `hyprctl dispatch movecursor` was the only lever,
        // and IT warps the cursor without generating that motion event.
        // Proved directly: warping off the vol cell to another cell on the
        // same bar surface leaves the vol cell still underlined, i.e. its
        // hover never cleared. So enter/leave ACROSS surfaces was testable
        // that way and motion WITHIN one was not, which is why the
        // cell-to-card handoff below was proven and the per-row hover wash
        // was not — worth re-running with the newer tool rather than assumed
        // still true. It is also the reason this design is preferred over
        // per-row MouseAreas even though both work under a real pointer: one
        // item that owns both the hover and the press has no delivery
        // question left to get wrong.
        //
        // `topMargin: -4` claims the host gap: StelePopout parents its child
        // at `anchors.topMargin: 4`, so 4px of popup surface sit ABOVE this
        // stele, directly under the bar cell the pointer is leaving, belonging
        // to no item — left unclaimed, a pointer that PAUSES there outlives
        // the grace timer and the card closes under it with nothing left to
        // reopen it. The cost of that margin is that `mouseY` is 4px ahead of
        // this root's own coordinates, which is why `rowAt` is handed
        // `mouseY - 4` and not `mouseY`.
        MouseArea {
            id: cueMa
            anchors.fill: parent
            anchors.topMargin: -4
            hoverEnabled: true
            cursorShape: root.cueHotRow >= 0 ? Qt.PointingHandCursor
                                             : Qt.ArrowCursor
            onEntered: { root.cueBodyHover = true; root.cueHotRow = cue.rowAt(mouseY - 4) }
            onPositionChanged: root.cueHotRow = cue.rowAt(mouseY - 4)
            onExited: { root.cueBodyHover = false; root.cueHotRow = -1 }
            onClicked: {
                var r = cue.rowAt(mouseY - 4)
                if (r >= 0) cue.channels()[r].picked()
            }
            // khoa, 2026-08-16 — the cue takes the WHEEL too. It rides this
            // same single object rather than a handler per row: the row is
            // already resolved arithmetically by the call `onClicked` uses, so
            // a wheel handler inside a row would reintroduce exactly the
            // nested-delivery problem the four rounds above eliminated. ±2 is
            // the bar cells' own step, not a new one.
            //
            // The bt row is left ALONE rather than swallowed: there is nothing
            // sensible to scroll on a power toggle, and eating the gesture
            // there would mean the wheel silently does nothing over a third of
            // the card with no way to tell that apart from a broken handler.
            // Unaccepted, it falls through to whatever is behind.
            onWheel: function(wheel) {
                // same angleDelta.y !== 0 gate as the bar cells' own wheel
                // handlers — a horizontal-only event no longer masquerades as
                // "turn it down" on either row. The bt row's passthrough
                // (wheel.accepted = false) is untouched.
                var r = cue.rowAt(mouseY - 4)
                if (r === 0) {
                    if (wheel.angleDelta.y !== 0) root.volAdjust(wheel.angleDelta.y > 0 ? 2 : -2)
                    wheel.accepted = true
                } else if (r === 1) {
                    if (wheel.angleDelta.y !== 0) root.micAdjust(wheel.angleDelta.y > 0 ? 2 : -2)
                    wheel.accepted = true
                } else wheel.accepted = false
            }
        }

        // cast shadow ───────────────────────────────────────────────────────
        Rectangle {
            anchors.fill: stele
            anchors.leftMargin: 4; anchors.topMargin: 5
            anchors.rightMargin: -4; anchors.bottomMargin: -5
            radius: 0
            color: cue.withA(cue.ink, 0.22)
        }

        Rectangle {
            id: stele
            width: parent.width
            anchors.top: parent.top
            radius: 0
            color: cue.livery.paletteBg
            border.color: cue.ink
            border.width: 2
            height: body.implicitHeight + 20

            Rectangle {                                   // inset amber keyline
                anchors.fill: parent; anchors.margins: 4
                radius: 0; color: "transparent"
                border.color: cue.sig; border.width: 1
            }

            Column {
                id: body
                anchors { left: parent.left; right: parent.right; top: parent.top }
                anchors.margins: 10
                spacing: 4

                // ── ENTABLATURE ────────────────────────────────────────────
                Item {
                    width: parent.width; height: 22
                    Text {
                        id: cueCrown
                        anchors.left: parent.left
                        anchors.verticalCenter: parent.verticalCenter
                        anchors.verticalCenterOffset: -1
                        text: "♪"; font.family: cue.faceMusic; font.pixelSize: 20
                        color: cue.sig
                    }
                    Text {
                        anchors.left: cueCrown.right; anchors.leftMargin: 8
                        anchors.verticalCenter: parent.verticalCenter
                        text: "CUE"
                        font.family: cue.faceSerif; font.pixelSize: 13
                        font.weight: Font.DemiBold; font.letterSpacing: 3
                        color: cue.ink
                    }
                    Text {
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        text: "[ ossia ]"
                        font.family: cue.faceMono; font.pixelSize: 10
                        color: cue.withA(cue.sig, 0.9)
                    }
                }

                // ── FRIEZE — a cue-staff course, three ruled lines ──────────
                Canvas {
                    id: cueFrieze
                    width: parent.width; height: 10
                    onWidthChanged: requestPaint()
                    onPaint: {
                        var ctx = getContext("2d")
                        ctx.reset(); ctx.clearRect(0, 0, width, height)
                        var alphas = [0.75, 0.45, 0.28]
                        ctx.lineWidth = 1
                        for (var i = 0; i < 3; i++) {
                            ctx.strokeStyle = cue.withA(cue.sig, alphas[i])
                            var y = 1.5 + i * 4
                            ctx.beginPath(); ctx.moveTo(0, y); ctx.lineTo(width, y); ctx.stroke()
                        }
                    }
                }

                // ── box-drawing top frame ──────────────────────────────────
                Item {
                    width: parent.width; height: 15
                    Text {
                        id: cueTfL
                        anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter
                        text: "┌─┤ now ├"
                        font.family: cue.faceMono; font.pixelSize: 11
                        color: cue.withA(cue.sig, 0.95)
                    }
                    Text {
                        id: cueTfR
                        anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                        text: "┐"; font.family: cue.faceMono; font.pixelSize: 11
                        color: cue.withA(cue.sig, 0.95)
                    }
                    Rectangle {
                        anchors.left: cueTfL.right; anchors.right: cueTfR.left
                        anchors.leftMargin: 2; anchors.rightMargin: 2
                        anchors.verticalCenter: parent.verticalCenter
                        anchors.verticalCenterOffset: 1
                        height: 1; color: cue.withA(cue.sig, 0.55)
                    }
                }

                // ── THE BODY — the three channels ────────────────────────────
                // khoa, 2026-08-16 — each row is a one-click silencer now:
                // out and in toggle mute, bt toggles adapter power. Powered
                // down and silenced are ONE state in this widget's grammar
                // (the colonnade draws both as the same broken column), so a
                // single word covers all three in the ledger below.
                CueChannel {
                    id: channelOut
                    livery: cue.livery
                    row: 0
                    glyph: root.volMuted ? "𝄽" : root.volIcon(root.volPct)
                    glyphFace: cue.faceMusic
                    name: "out"
                    hue: cue.livery.paletteAccent
                    live: root.volAvail
                    meter: root.volAvail && !root.volMuted
                    pct: root.volPct
                    value: !root.volAvail ? "—" : (root.volMuted ? "muted" : root.volPct + "%")
                    actionable: root.volAvail
                    hushed: root.volAvail && root.volMuted
                    onPicked: root.volToggleMute()
                }
                CueChannel {
                    id: channelIn
                    livery: cue.livery
                    row: 1
                    glyph: root.micMuted ? "𝄽" : "●"
                    glyphFace: root.micMuted ? cue.faceMusic : cue.faceMono
                    name: "in"
                    hue: cue.livery.holoBlue
                    live: root.micAvail
                    meter: root.micAvail && !root.micMuted
                    pct: root.micPct
                    value: !root.micAvail ? "—" : (root.micMuted ? "muted" : root.micPct + "%")
                    actionable: root.micAvail
                    hushed: root.micAvail && root.micMuted
                    onPicked: root.micToggleMute()
                }
                CueChannel {
                    id: channelBt
                    glyph: ""                       // nf-fa-bluetooth
                    livery: cue.livery
                    row: 2
                    glyphFace: cue.faceMono
                    name: "bt"
                    hue: cue.livery.wireCyan
                    // `live` is the UNAVAILABLE treatment (the row recedes to
                    // 0.4, §9), and a powered-DOWN adapter is not unavailable —
                    // it is the one row whose click matters most, since the
                    // click is what powers it on. So only "no adapter" recedes;
                    // the off state is already carried by the terracotta value
                    // and needs no second telling.
                    live: root.btAvail
                    chip: root.btProfile === "a2dp" ? "a2dp"
                        : (root.btProfile === "hsp" ? "hsp" : "")
                    value: !root.btAvail ? "—"
                         : (!root.btOn ? "off"
                         : (root.btConnected ? root.btShortName(12) : "on"))
                    actionable: root.btAvail
                    hushed: root.btAvail && !root.btOn
                    onPicked: root.btTogglePower()
                }

                // ── ledger line ────────────────────────────────────────────
                Item {
                    width: parent.width; height: 16
                    Rectangle {
                        anchors.top: parent.top
                        anchors.left: parent.left; anchors.right: parent.right
                        height: 1; color: cue.withA(cue.sig, 0.3)
                    }
                    Text {
                        anchors.left: parent.left; anchors.bottom: parent.bottom
                        text: cue.kaomojiFor()
                        font.pixelSize: 11
                        color: cue.withA(cue.sig, 0.9)
                    }
                    Text {
                        anchors.right: parent.right; anchors.bottom: parent.bottom
                        // khoa, 2026-08-16: was `mmb · mute`, which advertised
                        // a gesture on a DIFFERENT widget (the bar cell). That
                        // gesture is withdrawn and this line now names the one
                        // this card actually has. "silence" and not "mute"
                        // because the bt row cuts power rather than muting, and
                        // this widget already treats the two as one state.
                        // Two commands as of the wheel landing, in the colonnade
                        // ledger's own `scroll · set   click · mute` form.
                        text: "scroll · set  click · silence"
                        // 9 → 8: two commands do not fit beside the kaomoji at 9
                        // in a 217-wide card (they touched — captured). A hint
                        // is an informational layer, where the 7–9 micro tier
                        // is legal; nothing you came to read lives here.
                        font.family: cue.faceMono; font.pixelSize: 8
                        color: cue.withA(cue.ink, 0.5)
                    }
                }

                // ── closing frame — the live sink, ending on 𝄂 in gold ─────
                Item {
                    width: parent.width; height: 16
                    Text {
                        id: cueFfL
                        anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter
                        text: "└─┤ " + cue.sinkName() + " ├"
                        font.family: cue.faceMono; font.pixelSize: 10
                        color: cue.withA(cue.ink, 0.8)
                    }
                    Text {
                        id: cueFfCorner
                        anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                        text: "┘"; font.family: cue.faceMono; font.pixelSize: 11
                        color: cue.withA(cue.sig, 0.95)
                    }
                    Text {
                        id: cueFfBar
                        anchors.right: cueFfCorner.left; anchors.rightMargin: 4
                        anchors.verticalCenter: parent.verticalCenter
                        text: "𝄂"; font.family: cue.faceMusic; font.pixelSize: 15
                        color: cue.livery.paletteAccent
                    }
                    Rectangle {
                        anchors.left: cueFfL.right; anchors.right: cueFfBar.left
                        anchors.leftMargin: 2; anchors.rightMargin: 6
                        anchors.verticalCenter: parent.verticalCenter
                        anchors.verticalCenterOffset: 1
                        height: 1; color: cue.withA(cue.sig, 0.55)
                    }
                }
            }
        }
    }

    // ══ POPOUTS — real PopupWindows under their bar cells (BarPopout) ══════
    // Each is its own xdg_popup with GadgetFrame chrome (glass via blur_popups).
    // The cell ids above (volText/battText) anchor them. Meters/power/clock
    // popouts moved to the AoideAgentWidgets dock (bottom-seated frames).

    // The CUE — the hover readout. A small self-framed stele (the eleven parts
    // at their floor sizes, making-a-widget.md §1) hung under the vol cell,
    // reading out the three channels and nothing else. Hosted BARE through
    // StelePopout, per khoa's 2026-07-31 standing direction — NOT the
    // BarPopout/GadgetFrame the battery gauge still wears one cell over: that
    // host supplies its own pediment and closure, which would double-frame a
    // stele (live-confirmed on the calendar). The battery's BarPopout is the
    // house's remaining legacy host, explicitly "not the default for something
    // new" (widget-structure.md §4), so it is precedent for the BEHAVIOUR
    // (a small hover readout beside a bar cell) and not for the CHROME.
    StelePopout {
        cell: volText
        shown: root.audioCueShown && (root.volAvail || root.micAvail)
        AudioCue { livery: root.livery }
    }

    // Audio control — the three-bay colonnade (MIC · VOL · BT), a self-framed
    // marble stele hosted BARE (StelePopout, no GadgetFrame — it draws its own
    // chrome). CLICK-latched as of 2026-08-15: opened by clicking either audio
    // cell, closed by clicking it again. It no longer hover-retains, so the
    // body's `hovered` read is no longer wired to anything here.
    StelePopout {
        cell: volText
        shown: root.audioOpen && (root.volAvail || root.micAvail)
        // The colonnade RESIZES while open now (porch ↔ parthenon), so it takes
        // the anchor override StelePopout's header describes. It grows DOWNWARD
        // only — the width never changes — so the pin is about the horizontal
        // edge staying put across the two height steps rather than about the
        // width twitch: this cell sits near the right screen edge, so the
        // popup hangs from its RIGHT edge and opens leftward (edges
        // Bottom|Right, gravity Bottom|Left) instead of being re-centred.
        anchorEdges: Edges.Bottom | Edges.Right
        anchorGravity: Edges.Bottom | Edges.Left
        AudioColonnade {
            livery: root.livery
            outPct: root.volPct
            outMuted: root.volMuted
            outAvail: root.volAvail
            inPct: root.micPct
            inMuted: root.micMuted
            inAvail: root.micAvail
            btAvail: root.btAvail
            btOn: root.btOn
            btConnected: root.btConnected
            btName: root.btName
            btProfile: root.btProfile
            sinkRoster: root.sinkRoster
            sourceRoster: root.sourceRoster
            btRoster: root.btRoster
            streamRoster: root.streamRoster
            onStreamToggle: function(k) { root.streamToggle(k) }
            onStreamAdjust: function(k, d) { root.streamAdjust(k, d) }
            onPickSink: function(k) { root.pickSink(k) }
            onPickSource: function(k) { root.pickSource(k) }
            onPickBt: function(k) { root.pickBtDevice(k) }
            onOutToggle: root.volToggleMute()
            onOutAdjust: function(d) { root.volAdjust(d) }
            onInToggle: root.micToggleMute()
            onInAdjust: function(d) { root.micAdjust(d) }
            onBtToggle: root.btTogglePower()
            onBtPick: function(p) { root.btPickProfile(p) }
            onOpenMixer: root.openMixer()
        }
    }

    // Battery hover popout — time-remaining + charge bar.
    BarPopout {
        livery: root.livery
        cell: battText
        title: "battery.gauge"
        popoutWidth: 180
        shown: root.battShown && root.battAvail
        Column {
            width: parent.width
            spacing: 2
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                text: root.battBar(root.battPct) + " " + root.battPct + "%"
                color: root.battWarn ? root.livery.paletteUrgent : root.livery.paletteFg
                font.family: "monospace"
                font.pixelSize: 14
            }
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                visible: root.battTime().length > 0
                text: root.battTime()
                color: root.livery.paletteFg
                opacity: 0.75
                font.family: "monospace"
                font.pixelSize: 11
            }
        }
    }

    // ── System tray — the held items, a self-framed marble stele ───────────
    // Toggled by the fermata cell above (root.trayOpen); force-closed at zero
    // items by _recountTray(). Hosted BARE via StelePopout — khoa's 2026-07-31
    // standing direction for new bar popouts (StelePopout.qml's own header);
    // the torn-out first build's BarPopout bay would double-frame a
    // self-framed stele (live-confirmed on the calendar).
    //
    //   · ORDER     — the shared eleven-part stele at the shared sizes
    //                 (design/making-a-widget.md §1); the body is one
    //                 manuscript-ruled line per held item — the launcher's
    //                 row idiom (hairline rule, margin dot, icon, serif name,
    //                 mono gloss), no invented layout system.
    //   · SIGNATURE — verdigris (livery.wireCyan): the one role no popout
    //                 stele wears as its own (rust took notifications, amber
    //                 took audio + calendar). Full strength is legal as a
    //                 signature — the 0.5 alpha cap binds the STRUCTURAL
    //                 role, not the hue (design/greek-grammar.md §5).
    //   · CROWN     — 𝄋 segno, the return-sign, standing in where no clef
    //                 fits. Deliberately NOT 𝄐: the fermata is the toggle's
    //                 glyph, and 𝄐 serves THIS stele as the awaiting mark on
    //                 an attention row (state contract, greek-grammar.md §3)
    //                 — two registers of one glyph may not collide in one
    //                 place. 𝄋 already renders live in this file's workspace
    //                 row (the scratch workspace).
    //   · FRIEZE    — a fermata course: repeated hold-marks (arc over dot),
    //                 Canvas, 10px, lineWidth 1.2, in the signature — the
    //                 popout's token (`tray.held`) made ornament, the one
    //                 place the concept shows.
    //   · STATES    — hover is the ONE laurel: rule 1px to 2px + margin
    //                 mark · to ♪ (two reads, one state, the launcher's
    //                 selection). Status.NeedsAttention is the awaiting
    //                 treatment: 𝄐 mark + terracotta title + Bold + the
    //                 600ms 0.35/1.0 pulse — weight and motion carry the
    //                 state with colour removed.
    //
    // Wiring, grounded: the Repeater binds SystemTray.items (the ObjectModel)
    // directly, never a .values.slice() copy (design/hazards.md §4). Left
    // click → activate() — UNLESS the item is menu-only (onlyMenu, the
    // appindicator/dbusmenu breed: nm-applet is one), where Activate is not
    // implemented at all and the old unconditional activate() clicked into
    // silence (the User's 2026-08-23 "clicking it does nothing"). Menu-only
    // items open their dbusmenu through a QsMenuAnchor instead; right click
    // prefers the menu whenever one exists, falling back to
    // secondaryActivate(). This Quickshell rev exposes no tertiaryActivate.
    // modelData.icon is already an image URL, rendered straight in an Image.
    // Every rare glyph rides a NAMED face at REGULAR weight — the fermata
    // pair's bold-monospace invisibility scar is documented on the toggle
    // above.
    StelePopout {
        cell: trayToggle
        shown: root.trayOpen && root.trayCount > 0

        Item {
            id: tstele
            width: 300
            implicitHeight: trayBody.height + 5   // +5 clears the cast shadow

            // type voices — shared across the pantheon (greek-grammar.md §1)
            readonly property string faceSerif: "Noto Serif"
            readonly property string faceMono:  "JetBrainsMono Nerd Font"
            readonly property string faceMusic: "Noto Music"

            // this stele's signature — VERDIGRIS (wireCyan)
            readonly property color sig: root.livery.wireCyan
            readonly property color ink: root.livery.paletteFg

            function withA(cstr, a) {
                var c = Qt.color(cstr)
                return Qt.rgba(c.r, c.g, c.b, a)
            }
            // one mood face reading the whole court, keyed on the live count
            // (kana/punctuation atoms MoodFaces already proves safe)
            function kaomojiFor() {
                if (root.trayCount >= 4) return "(˘ω˘ )"      // a full retinue, at ease
                if (root.trayCount >= 2) return "( ･ω･)ノ"    // a few attendants, alert
                return "( ･_･)ノ"                              // the lone attendant
            }

            // cast shadow — shared pantheon idiom
            Rectangle {
                anchors.fill: trayBody
                anchors.leftMargin: 4; anchors.topMargin: 5
                anchors.rightMargin: -4; anchors.bottomMargin: -5
                radius: 0
                color: tstele.withA(tstele.ink, 0.22)
            }

            // the stele — opaque marble body, 2px ink border, inset keyline
            Rectangle {
                id: trayBody
                width: parent.width
                anchors.top: parent.top
                radius: 0
                color: root.livery.paletteBg
                border.color: tstele.ink
                border.width: 2
                height: trayContent.implicitHeight + 20

                Rectangle {                        // inset verdigris keyline
                    anchors.fill: parent; anchors.margins: 4
                    radius: 0; color: "transparent"
                    border.color: tstele.sig; border.width: 1
                }

                Column {
                    id: trayContent
                    anchors { left: parent.left; right: parent.right; top: parent.top }
                    anchors.margins: 10
                    spacing: 4

                    // ── ENTABLATURE: crown + carved name + protocol tag ─────
                    Item {
                        width: parent.width; height: 24
                        Text {
                            id: trayCrown
                            anchors.left: parent.left
                            anchors.verticalCenter: parent.verticalCenter
                            text: "𝄋"
                            font.family: tstele.faceMusic
                            font.pixelSize: 22
                            color: tstele.sig
                        }
                        Text {
                            anchors.left: trayCrown.right; anchors.leftMargin: 8
                            anchors.verticalCenter: parent.verticalCenter
                            text: "RETINUE"
                            font.family: tstele.faceSerif
                            font.pixelSize: 13
                            font.weight: Font.DemiBold
                            font.letterSpacing: 3
                            color: tstele.ink
                        }
                        Text {
                            anchors.right: parent.right
                            anchors.verticalCenter: parent.verticalCenter
                            text: "[ sni ]"
                            font.family: tstele.faceMono
                            font.pixelSize: 10
                            color: tstele.withA(tstele.sig, 0.9)
                        }
                    }

                    // ── the fermata course — repeated hold-marks, verdigris ──
                    Canvas {
                        width: parent.width; height: 10
                        readonly property color tone: tstele.withA(tstele.sig, 0.85)
                        onToneChanged: requestPaint()
                        onWidthChanged: requestPaint()
                        onPaint: {
                            var ctx = getContext("2d")
                            ctx.reset(); ctx.clearRect(0, 0, width, height)
                            ctx.strokeStyle = tone
                            ctx.fillStyle = tone
                            ctx.lineWidth = 1.2
                            var cy = height - 2, period = 14
                            for (var x = 7; x < width - 7; x += period) {
                                ctx.beginPath()
                                ctx.arc(x, cy, 4.4, Math.PI, 2 * Math.PI)  // the hold's arc
                                ctx.stroke()
                                ctx.beginPath()
                                ctx.arc(x, cy - 1.4, 1.1, 0, 2 * Math.PI)  // the dot beneath
                                ctx.fill()
                            }
                        }
                    }

                    // ── box-drawing top frame — the popout's token ──────────
                    Item {
                        width: parent.width; height: 15
                        Text {
                            id: trayTfL
                            anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter
                            text: "┌─┤ tray.held ├"
                            font.family: tstele.faceMono; font.pixelSize: 11
                            color: tstele.withA(tstele.sig, 0.95)
                        }
                        Text {
                            id: trayTfR
                            anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                            text: "┐"; font.family: tstele.faceMono; font.pixelSize: 11
                            color: tstele.withA(tstele.sig, 0.95)
                        }
                        Rectangle {
                            anchors.left: trayTfL.right; anchors.right: trayTfR.left
                            anchors.leftMargin: 2; anchors.rightMargin: 2
                            anchors.verticalCenter: parent.verticalCenter
                            anchors.verticalCenterOffset: 1
                            height: 1; color: tstele.withA(tstele.sig, 0.55)
                        }
                    }

                    // ── THE BODY — one manuscript-ruled line per held item ──
                    Column {
                        width: parent.width
                        Repeater {
                            model: SystemTray.items
                            delegate: Item {
                                id: trayRow
                                required property var modelData
                                width: parent ? parent.width : 0
                                height: 34

                                readonly property bool attn:
                                    trayRow.modelData
                                    && trayRow.modelData.status === Status.NeedsAttention
                                readonly property bool hovered: rowMa.containsMouse

                                // Data shaping (the notification card's rule:
                                // go get the tiers) — title falls back to the
                                // item id; the gloss is the first of tooltip
                                // title/description/id that adds NEW ink, so
                                // nothing reads twice.
                                readonly property string label: {
                                    var m = trayRow.modelData
                                    var t = (m && m.title) ? ("" + m.title).trim() : ""
                                    if (t.length > 0) return t
                                    var i = (m && m.id) ? ("" + m.id).trim() : ""
                                    return i.length > 0 ? i : "item"
                                }
                                readonly property string gloss: {
                                    var m = trayRow.modelData
                                    if (!m) return ""
                                    var cands = [m.tooltipTitle, m.tooltipDescription, m.id]
                                    for (var k = 0; k < cands.length; k++) {
                                        var s = cands[k] ? ("" + cands[k]).trim() : ""
                                        if (s.length > 0 && s !== trayRow.label) return s
                                    }
                                    return ""
                                }

                                // the ruling — ignites laurel on hover
                                Rectangle {
                                    anchors.left: parent.left; anchors.right: parent.right
                                    anchors.bottom: parent.bottom
                                    height: trayRow.hovered ? 2 : 1
                                    color: trayRow.hovered ? root.livery.paletteHot
                                                           : tstele.withA(tstele.ink, 0.13)
                                    Behavior on color { ColorAnimation { duration: 150 } }
                                }

                                Row {
                                    anchors.left: parent.left
                                    anchors.right: parent.right
                                    anchors.verticalCenter: parent.verticalCenter
                                    anchors.verticalCenterOffset: -1
                                    spacing: 8

                                    Text {   // margin mark: · rest / ♪ hover / 𝄐 attention
                                        id: rowMark
                                        anchors.verticalCenter: parent.verticalCenter
                                        width: 14
                                        horizontalAlignment: Text.AlignHCenter
                                        text: trayRow.attn ? "𝄐"
                                              : trayRow.hovered ? "♪" : "·"
                                        font.family: tstele.faceMusic
                                        font.pixelSize: trayRow.hovered && !trayRow.attn ? 15 : 13
                                        color: trayRow.attn ? root.livery.paletteUrgent
                                               : trayRow.hovered ? root.livery.paletteHot
                                               : tstele.withA(tstele.ink, 0.3)
                                        SequentialAnimation on opacity {
                                            running: trayRow.attn
                                            loops: Animation.Infinite
                                            alwaysRunToEnd: true
                                            NumberAnimation { to: 0.35; duration: 600; easing.type: Easing.InOutQuad }
                                            NumberAnimation { to: 1.0;  duration: 600; easing.type: Easing.InOutQuad }
                                        }
                                    }
                                    Image {
                                        anchors.verticalCenter: parent.verticalCenter
                                        width: 20; height: 20
                                        sourceSize.width: 20; sourceSize.height: 20
                                        fillMode: Image.PreserveAspectFit
                                        source: (trayRow.modelData && trayRow.modelData.icon)
                                                ? trayRow.modelData.icon : ""
                                    }
                                    Text {
                                        id: rowName
                                        anchors.verticalCenter: parent.verticalCenter
                                        text: trayRow.label
                                        color: trayRow.attn ? root.livery.paletteUrgent
                                                            : tstele.ink
                                        opacity: trayRow.hovered ? 1.0 : 0.88
                                        font.family: tstele.faceSerif
                                        font.pixelSize: 14
                                        font.weight: (trayRow.attn || trayRow.hovered)
                                                     ? Font.Bold : Font.Medium
                                        elide: Text.ElideRight
                                        width: Math.min(implicitWidth, parent.width * 0.55)
                                    }
                                    Text {   // informational gloss — aegean, never chrome
                                        anchors.verticalCenter: parent.verticalCenter
                                        anchors.verticalCenterOffset: 1
                                        visible: trayRow.gloss.length > 0
                                        text: trayRow.gloss
                                        color: tstele.withA(root.livery.holoBlue, 0.8)
                                        font.family: tstele.faceMono
                                        font.pixelSize: 10
                                        elide: Text.ElideRight
                                        width: Math.max(0, parent.width - rowMark.width
                                                        - 20 - rowName.width - 24)
                                    }
                                }

                                // The item's dbusmenu, hung under this row.
                                // anchor.item gives QsMenuAnchor both the
                                // window (the StelePopout popup) and the
                                // rect; the menu opens as its own popup
                                // below the row.
                                QsMenuAnchor {
                                    id: rowMenu
                                    menu: trayRow.modelData ? trayRow.modelData.menu : null
                                    anchor.item: trayRow
                                    anchor.edges: Edges.Bottom
                                    anchor.gravity: Edges.Bottom
                                }

                                MouseArea {
                                    id: rowMa
                                    anchors.fill: parent
                                    hoverEnabled: true
                                    acceptedButtons: Qt.LeftButton | Qt.RightButton
                                    cursorShape: Qt.PointingHandCursor
                                    // Menu-first on BOTH buttons when a menu
                                    // exists: onlyMenu (ItemIsMenu) cannot be
                                    // trusted as the discriminator — nm-applet
                                    // publishes a Menu path and NO ItemIsMenu
                                    // property at all (busctl-verified live,
                                    // 2026-08-23), and libayatana items never
                                    // implement Activate, so gating the menu
                                    // on onlyMenu re-created the exact silent
                                    // click this fixes. An item with no menu
                                    // still gets its activate pair.
                                    onClicked: function(mouse) {
                                        var m = trayRow.modelData
                                        if (!m) return
                                        if (m.hasMenu) {
                                            rowMenu.open()
                                        } else if (mouse.button === Qt.RightButton) {
                                            m.secondaryActivate()
                                        } else {
                                            m.activate()
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // ── ledger: kaomoji + the interaction hint ──────────────
                    Item {
                        width: parent.width; height: 16
                        Rectangle {
                            anchors.top: parent.top
                            anchors.left: parent.left; anchors.right: parent.right
                            height: 1; color: tstele.withA(tstele.sig, 0.3)
                        }
                        Text {
                            anchors.left: parent.left; anchors.bottom: parent.bottom
                            text: tstele.kaomojiFor()
                            font.pixelSize: 11
                            color: tstele.withA(tstele.sig, 0.9)
                        }
                        Text {
                            anchors.right: parent.right; anchors.bottom: parent.bottom
                            text: "click · open   right · menu"
                            font.family: tstele.faceMono; font.pixelSize: 9
                            color: tstele.withA(tstele.ink, 0.5)
                        }
                    }

                    // ── box-drawing bottom frame — live count, closing 𝄂 ────
                    Item {
                        width: parent.width; height: 18
                        Text {
                            id: trayFfL
                            anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter
                            text: "└─┤ " + root.trayCount + " held ├"
                            font.family: tstele.faceMono; font.pixelSize: 11
                            color: tstele.withA(tstele.ink, 0.8)
                        }
                        Text {
                            id: trayFfCorner
                            anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                            text: "┘"; font.family: tstele.faceMono; font.pixelSize: 11
                            color: tstele.withA(tstele.sig, 0.95)
                        }
                        Text {
                            id: trayFfBar
                            anchors.right: trayFfCorner.left; anchors.rightMargin: 4
                            anchors.verticalCenter: parent.verticalCenter
                            text: "𝄂"; font.family: tstele.faceMusic; font.pixelSize: 16
                            color: root.livery.paletteAccent
                        }
                        Rectangle {
                            anchors.left: trayFfL.right; anchors.right: trayFfBar.left
                            anchors.leftMargin: 2; anchors.rightMargin: 6
                            anchors.verticalCenter: parent.verticalCenter
                            anchors.verticalCenterOffset: 1
                            height: 1; color: tstele.withA(tstele.sig, 0.55)
                        }
                    }
                }
            }
        }
    }

    // ── DIKTYON — the network stele (native NM picker) ─────────────────────
    // Toggled by the network cell (root.netOpen). Hosted BARE via StelePopout
    // (khoa's 2026-07-31 standing direction). Born 2026-08-23 from the User's
    // "clicking it does nothing, and I cannot quick add networks/discover
    // them": the answer is not nm-applet's GTK menu but a native stele over
    // Quickshell.Networking — discovery (live scan while open), quick-join
    // (click a row; a secured unknown row unrolls an inline PSK line), and
    // the wifi kill-switch, all in the house grammar. nm-applet stays in the
    // RETINUE for what the native API doesn't reach (VPNs, hidden-SSID add) —
    // its dbusmenu now actually opens from there.
    //
    //   · ORDER     — the shared eleven-part stele at the shared sizes
    //                 (design/making-a-widget.md §1); the body is one
    //                 manuscript-ruled line per heard network — the
    //                 launcher/tray row idiom, no invented layout.
    //   · SIGNATURE — clay (livery.base09). Murex went to the colonnade
    //                 (2026-08-16), verdigris to the tray, rust to the
    //                 herald; clay held two steles at once until the
    //                 colonnade moved off it, so the sharing precedent is
    //                 clay's own (calendar keeps it too). Aegean was the
    //                 poetic pick (δίκτυον, the fisherman's net) and is
    //                 REJECTED: the grammar caps holoBlue at information,
    //                 never chrome (greek-grammar.md §5).
    //   · CROWN     — 𝄌 coda: the jump-mark that binds two distant points
    //                 of a score into one performance — a network. Renders
    //                 live in powermenu.qml (LOGOUT) at this face.
    //   · FRIEZE    — a beacon course: repeated three-arc signal fans,
    //                 Canvas, 10px, lineWidth 1.2, in the signature.
    //   · STATES    — signal strength is spoken in DYNAMICS (𝆏𝆏…𝆑𝆑, the
    //                 meters.qml scale, proven on screen); the connected
    //                 row's name takes the accent (open/active/live);
    //                 a connecting row wears 𝄐 + the 600ms pulse (the
    //                 awaiting register); hover is the one laurel (rule +
    //                 margin ♪, the launcher/tray selection).
    //
    // Grounded: Networking.devices/WifiDevice/WifiNetwork per the compiled
    // quickshell-network.qmltypes (connect/connectWithPsk/forget/disconnect,
    // connectionFailed(NoSecrets) → the PSK line opens itself). The sorted
    // list rides a ScriptModel — it diffs by object identity, so a re-sort
    // does not tear down every delegate the way a bare .values.slice() model
    // would (hazards.md §4). Silent property changes (strength/connected/
    // known) re-sort through a zero-size watcher Repeater bound to the
    // ObjectModel directly — the Bluetooth block's idiom.
    StelePopout {
        cell: netText
        shown: root.netOpen

        Item {
            id: nstele
            width: 320
            implicitHeight: netBody.height + 5   // +5 clears the cast shadow

            // type voices — shared across the pantheon (greek-grammar.md §1)
            readonly property string faceSerif: "Noto Serif"
            readonly property string faceMono:  "JetBrainsMono Nerd Font"
            readonly property string faceMusic: "Noto Music"

            // this stele's signature — CLAY (base09)
            readonly property color sig: root.livery.base09
            readonly property color ink: root.livery.paletteFg

            function withA(cstr, a) {
                var c = Qt.color(cstr)
                return Qt.rgba(c.r, c.g, c.b, a)
            }

            // ── data shaping ────────────────────────────────────────────
            property var wifiSorted: []
            property var pskTarget: null    // WifiNetwork the PSK line asks for
            property string lastFail: ""    // one line, terracotta, cleared on retry

            function resort() {
                var dev = root.wifiDev
                var vals = (dev && dev.networks && dev.networks.values)
                           ? dev.networks.values : []
                var arr = vals.slice()
                arr.sort(function(a, b) {
                    if (a.connected !== b.connected) return a.connected ? -1 : 1
                    if (a.known !== b.known) return a.known ? -1 : 1
                    return b.signalStrength - a.signalStrength
                })
                wifiSorted = arr
            }
            // NM reports strength as 0–100; the binding docs say double, so
            // normalize defensively instead of trusting either convention.
            function pct(s) {
                var v = s <= 1.0 ? s * 100 : s
                return Math.max(0, Math.min(100, Math.round(v)))
            }
            function dynGlyph(p) {           // the meters.qml dynamics scale
                if (p >= 85) return "𝆑𝆑"
                if (p >= 65) return "𝆑"
                if (p >= 45) return "𝆐"
                if (p >= 25) return "𝆏"
                return "𝆏𝆏"
            }
            function secWord(s) {
                if (s === WifiSecurityType.Open) return "open"
                if (s === WifiSecurityType.Owe) return "owe"
                if (s === WifiSecurityType.Sae
                    || s === WifiSecurityType.Wpa3SuiteB192) return "wpa3"
                if (s === WifiSecurityType.Wpa2Psk
                    || s === WifiSecurityType.Wpa2Eap) return "wpa2"
                if (s === WifiSecurityType.WpaPsk
                    || s === WifiSecurityType.WpaEap) return "wpa"
                return "wep"
            }
            function secured(s) {
                return s !== WifiSecurityType.Open && s !== WifiSecurityType.Owe
            }
            function openPsk(n) { pskTarget = n }
            function clearPsk() { pskTarget = null }
            function connWord() {
                var c = Networking.connectivity
                if (c === NetworkConnectivity.Full)    return "full"
                if (c === NetworkConnectivity.Limited) return "limited"
                if (c === NetworkConnectivity.Portal)  return "portal"
                if (c === NetworkConnectivity.None)    return "cut"
                return "?"
            }
            // one mood face, keyed on the live connectivity read
            function kaomojiFor() {
                if (!Networking.wifiEnabled && !root.wiredDev) return "( ´ω` )zZ"
                var c = Networking.connectivity
                if (c === NetworkConnectivity.Full)    return "( ｀･ω･´)"
                if (c === NetworkConnectivity.None)    return "( ´･ω･` )"
                return "( ･ω･ )?"
            }

            // re-sort on arrivals/departures…
            Connections {
                target: root.wifiDev ? root.wifiDev.networks : null
                function onObjectInsertedPost(obj, index) { nstele.resort() }
                function onObjectRemovedPost(obj, index)  { nstele.resort() }
            }
            // …and on the changes the model is silent about (strength sways,
            // a join lands) — the Bluetooth watcher idiom.
            Repeater {
                model: root.wifiDev ? root.wifiDev.networks : null
                delegate: Item {
                    required property var modelData
                    width: 0; height: 0; visible: false
                    readonly property real sway: modelData ? modelData.signalStrength : 0
                    readonly property bool conn: modelData ? modelData.connected : false
                    readonly property bool kn:   modelData ? modelData.known : false
                    onSwayChanged: nstele.resort()
                    onConnChanged: nstele.resort()
                    onKnChanged:   nstele.resort()
                    Component.onCompleted: nstele.resort()
                }
            }
            // closing the stele folds the PSK line and drops the fail note
            Connections {
                target: root
                function onNetOpenChanged() {
                    if (!root.netOpen) { nstele.clearPsk(); nstele.lastFail = "" }
                }
            }
            // Keyboard reaches an xdg_popup only under a compositor focus
            // grab — the bar's layer surface never takes keys, and without
            // this every PSK keystroke would land in whatever toplevel held
            // focus (verified reachable: no keyboardFocus anywhere in the
            // popout chain). Grab exactly while a PSK line is open; a click
            // anywhere outside clears the grab, which folds the line — the
            // dismiss gesture falls out of the same mechanism.
            HyprlandFocusGrab {
                windows: [ nstele.QsWindow.window ]
                active: nstele.pskTarget !== null
                onCleared: nstele.clearPsk()
            }

            // cast shadow — shared pantheon idiom
            Rectangle {
                anchors.fill: netBody
                anchors.leftMargin: 4; anchors.topMargin: 5
                anchors.rightMargin: -4; anchors.bottomMargin: -5
                radius: 0
                color: nstele.withA(nstele.ink, 0.22)
            }

            // the stele — opaque marble body, 2px ink border, inset keyline
            Rectangle {
                id: netBody
                width: parent.width
                anchors.top: parent.top
                radius: 0
                color: root.livery.paletteBg
                border.color: nstele.ink
                border.width: 2
                height: netContent.implicitHeight + 20

                Rectangle {                        // inset clay keyline
                    anchors.fill: parent; anchors.margins: 4
                    radius: 0; color: "transparent"
                    border.color: nstele.sig; border.width: 1
                }

                Column {
                    id: netContent
                    anchors { left: parent.left; right: parent.right; top: parent.top }
                    anchors.margins: 10
                    spacing: 4

                    // ── ENTABLATURE: crown + carved name + protocol tag ─────
                    Item {
                        width: parent.width; height: 24
                        Text {
                            id: netCrown
                            anchors.left: parent.left
                            anchors.verticalCenter: parent.verticalCenter
                            text: "𝄌"
                            font.family: nstele.faceMusic
                            font.pixelSize: 22
                            color: nstele.sig
                        }
                        Text {
                            anchors.left: netCrown.right; anchors.leftMargin: 8
                            anchors.verticalCenter: parent.verticalCenter
                            text: "DIKTYON"
                            font.family: nstele.faceSerif
                            font.pixelSize: 15
                            font.weight: Font.DemiBold
                            font.letterSpacing: 4
                            color: nstele.ink
                        }
                        Text {
                            anchors.right: parent.right
                            anchors.verticalCenter: parent.verticalCenter
                            text: "[ nm ]"
                            font.family: nstele.faceMono
                            font.pixelSize: 10
                            color: nstele.withA(nstele.sig, 0.9)
                        }
                    }

                    // ── FRIEZE: the beacon course — three-arc signal fans ───
                    Canvas {
                        width: parent.width; height: 10
                        onPaint: {
                            var ctx = getContext("2d")
                            ctx.reset()
                            ctx.strokeStyle = "" + nstele.sig
                            ctx.lineWidth = 1.2
                            for (var x = 8; x < width - 8; x += 26) {
                                for (var r = 3; r <= 9; r += 3) {
                                    ctx.beginPath()
                                    ctx.arc(x, height, r, Math.PI * 1.25, Math.PI * 1.75)
                                    ctx.stroke()
                                }
                            }
                        }
                        Component.onCompleted: requestPaint()
                        onWidthChanged: requestPaint()
                    }

                    // ── TOP FRAME — and the wifi kill-switch lives in it ────
                    // ┌─┤ wifi on ├────────────┐  · the ├ label ┤ is the one
                    // control this frame line carries: click flips
                    // Networking.wifiEnabled. Gold while on (open/active/
                    // live), dim ink while off — state readable sans colour
                    // by the word itself.
                    Item {
                        width: parent.width; height: 15
                        Text {
                            id: netTfL
                            anchors.left: parent.left
                            anchors.verticalCenter: parent.verticalCenter
                            text: "┌─┤"
                            font.family: nstele.faceMono; font.pixelSize: 11
                            color: nstele.withA(nstele.sig, 0.95)
                        }
                        Text {
                            id: wifiSwitch
                            anchors.left: netTfL.right; anchors.leftMargin: 5
                            anchors.verticalCenter: parent.verticalCenter
                            text: Networking.wifiEnabled ? "wifi on" : "wifi off"
                            font.family: nstele.faceMono; font.pixelSize: 11
                            font.underline: wsMa.containsMouse
                            color: Networking.wifiEnabled
                                   ? root.livery.paletteAccent
                                   : nstele.withA(nstele.ink, 0.5)
                            MouseArea {
                                id: wsMa
                                anchors.fill: parent
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                onClicked: Networking.wifiEnabled = !Networking.wifiEnabled
                            }
                        }
                        Text {
                            id: netTfL2
                            anchors.left: wifiSwitch.right; anchors.leftMargin: 5
                            anchors.verticalCenter: parent.verticalCenter
                            text: "├"
                            font.family: nstele.faceMono; font.pixelSize: 11
                            color: nstele.withA(nstele.sig, 0.95)
                        }
                        Text {
                            id: netTfR
                            anchors.right: parent.right
                            anchors.verticalCenter: parent.verticalCenter
                            text: "┐"
                            font.family: nstele.faceMono; font.pixelSize: 11
                            color: nstele.withA(nstele.sig, 0.95)
                        }
                        Rectangle {
                            anchors.left: netTfL2.right; anchors.right: netTfR.left
                            anchors.leftMargin: 2; anchors.rightMargin: 2
                            anchors.verticalCenter: parent.verticalCenter
                            anchors.verticalCenterOffset: 1
                            height: 1; color: nstele.withA(nstele.sig, 0.55)
                        }
                    }

                    // ── THE BODY ────────────────────────────────────────────
                    // the wired line first (steady ground under the air), then
                    // one manuscript-ruled line per heard wifi network.
                    Column {
                        width: parent.width

                        // wired — present only when a NIC exists; no click,
                        // nothing to choose. 𝆺𝅥𝅯 is the bar's own eth glyph.
                        Item {
                            visible: root.wiredDev !== null
                            width: parent.width; height: visible ? 30 : 0
                            Rectangle {
                                anchors.left: parent.left; anchors.right: parent.right
                                anchors.bottom: parent.bottom
                                height: 1; color: nstele.withA(nstele.ink, 0.13)
                            }
                            Row {
                                anchors.left: parent.left
                                anchors.right: parent.right
                                anchors.verticalCenter: parent.verticalCenter
                                spacing: 8
                                Text {
                                    anchors.verticalCenter: parent.verticalCenter
                                    width: 14; horizontalAlignment: Text.AlignHCenter
                                    text: "·"
                                    font.family: nstele.faceMusic; font.pixelSize: 13
                                    color: nstele.withA(nstele.ink, 0.3)
                                }
                                Text {
                                    anchors.verticalCenter: parent.verticalCenter
                                    width: 24
                                    text: "𝆺𝅥𝅯"
                                    font.family: nstele.faceMusic; font.pixelSize: 13
                                    color: (root.wiredDev && root.wiredDev.hasLink)
                                           ? nstele.ink : nstele.withA(nstele.ink, 0.4)
                                }
                                Text {
                                    anchors.verticalCenter: parent.verticalCenter
                                    text: root.wiredDev ? ("" + root.wiredDev.name) : ""
                                    font.family: nstele.faceSerif; font.pixelSize: 13
                                    font.weight: (root.wiredDev && root.wiredDev.connected)
                                                 ? Font.Bold : Font.Medium
                                    color: (root.wiredDev && root.wiredDev.connected)
                                           ? root.livery.paletteAccent : nstele.ink
                                    elide: Text.ElideRight
                                }
                            }
                            Text {
                                anchors.right: parent.right
                                anchors.verticalCenter: parent.verticalCenter
                                text: root.wiredDev
                                      ? (root.wiredDev.hasLink
                                         ? (root.wiredDev.linkSpeed > 0
                                            ? root.wiredDev.linkSpeed + " Mb/s" : "link")
                                         : "no link")
                                      : ""
                                font.family: nstele.faceMono; font.pixelSize: 10
                                color: nstele.withA(root.livery.holoBlue, 0.8)
                            }
                        }

                        // taught empty states — a rest must still read (§4)
                        Item {
                            visible: Networking.backend === NetworkBackendType.None
                            width: parent.width; height: visible ? 30 : 0
                            Text {
                                anchors.centerIn: parent
                                text: "no NetworkManager on this bus"
                                font.family: nstele.faceSerif; font.pixelSize: 12
                                font.italic: true
                                color: nstele.withA(nstele.ink, 0.5)
                            }
                        }
                        Item {
                            visible: Networking.backend !== NetworkBackendType.None
                                     && !Networking.wifiEnabled
                            width: parent.width; height: visible ? 30 : 0
                            Text {
                                anchors.centerIn: parent
                                text: "the air is switched off — wifi on above rekindles it"
                                font.family: nstele.faceSerif; font.pixelSize: 12
                                font.italic: true
                                color: nstele.withA(nstele.ink, 0.5)
                            }
                        }
                        Item {
                            visible: Networking.wifiEnabled
                                     && nstele.wifiSorted.length === 0
                                     && Networking.backend !== NetworkBackendType.None
                            width: parent.width; height: visible ? 30 : 0
                            Text {
                                anchors.centerIn: parent
                                text: "listening for beacons…"
                                font.family: nstele.faceSerif; font.pixelSize: 12
                                font.italic: true
                                color: nstele.withA(nstele.ink, 0.5)
                            }
                        }

                        // the heard networks
                        Repeater {
                            model: ScriptModel { values: nstele.wifiSorted }
                            delegate: Item {
                                id: netRow
                                required property var modelData
                                width: parent ? parent.width : 0

                                readonly property bool hovered: nrMa.containsMouse
                                readonly property bool conn:
                                    netRow.modelData && netRow.modelData.connected
                                readonly property bool busy:
                                    netRow.modelData && netRow.modelData.stateChanging
                                readonly property bool asking:
                                    nstele.pskTarget === netRow.modelData
                                readonly property int strength:
                                    netRow.modelData
                                    ? nstele.pct(netRow.modelData.signalStrength) : 0

                                height: asking ? 64 : 30
                                Behavior on height {
                                    NumberAnimation { duration: 140; easing.type: Easing.OutCubic }
                                }
                                clip: true

                                // NoSecrets → the PSK line opens itself; any
                                // failure inks one terracotta line below.
                                Connections {
                                    target: netRow.modelData
                                    function onConnectionFailed(reason) {
                                        var n = netRow.modelData
                                        nstele.lastFail = ("" + n.name) + " — "
                                            + ConnectionFailReason.toString(reason)
                                        if (reason === ConnectionFailReason.NoSecrets)
                                            nstele.openPsk(n)
                                    }
                                }

                                // the ruling — ignites laurel on hover
                                Rectangle {
                                    anchors.left: parent.left; anchors.right: parent.right
                                    anchors.bottom: parent.bottom
                                    height: netRow.hovered ? 2 : 1
                                    color: netRow.hovered ? root.livery.paletteHot
                                                          : nstele.withA(nstele.ink, 0.13)
                                    Behavior on color { ColorAnimation { duration: 150 } }
                                }

                                Row {
                                    id: nrLine
                                    anchors.left: parent.left
                                    anchors.right: parent.right
                                    anchors.top: parent.top
                                    height: 30
                                    spacing: 8

                                    Text {   // margin mark: · rest / ♪ hover / 𝄐 joining
                                        id: nrMark
                                        anchors.verticalCenter: parent.verticalCenter
                                        width: 14
                                        horizontalAlignment: Text.AlignHCenter
                                        text: netRow.busy ? "𝄐"
                                              : netRow.hovered ? "♪" : "·"
                                        font.family: nstele.faceMusic
                                        font.pixelSize: netRow.hovered && !netRow.busy ? 15 : 13
                                        color: netRow.busy ? root.livery.paletteUrgent
                                               : netRow.hovered ? root.livery.paletteHot
                                               : nstele.withA(nstele.ink, 0.3)
                                        SequentialAnimation on opacity {
                                            running: netRow.busy
                                            loops: Animation.Infinite
                                            alwaysRunToEnd: true
                                            NumberAnimation { to: 0.35; duration: 600; easing.type: Easing.InOutQuad }
                                            NumberAnimation { to: 1.0;  duration: 600; easing.type: Easing.InOutQuad }
                                        }
                                    }
                                    Text {   // strength, spoken in dynamics
                                        anchors.verticalCenter: parent.verticalCenter
                                        width: 24
                                        text: nstele.dynGlyph(netRow.strength)
                                        font.family: nstele.faceMusic
                                        font.pixelSize: 13
                                        color: nstele.withA(nstele.sig,
                                                   0.5 + 0.5 * (netRow.strength / 100))
                                    }
                                    Text {   // the SSID — gold when it's the one playing
                                        id: nrName
                                        anchors.verticalCenter: parent.verticalCenter
                                        text: netRow.modelData ? ("" + netRow.modelData.name) : ""
                                        color: netRow.conn ? root.livery.paletteAccent
                                                           : nstele.ink
                                        opacity: netRow.conn ? 1.0
                                                 : (netRow.modelData && netRow.modelData.known)
                                                   ? 0.95 : 0.75
                                        font.family: nstele.faceSerif
                                        font.pixelSize: 13
                                        font.weight: (netRow.conn || netRow.hovered)
                                                     ? Font.Bold : Font.Medium
                                        elide: Text.ElideRight
                                        width: Math.min(implicitWidth, parent.width * 0.5)
                                    }
                                    Text {   // informational gloss — aegean, never chrome
                                        anchors.verticalCenter: parent.verticalCenter
                                        anchors.verticalCenterOffset: 1
                                        text: {
                                            var n = netRow.modelData
                                            if (!n) return ""
                                            var bits = [nstele.secWord(n.security),
                                                        netRow.strength + ""]
                                            if (n.known && !n.connected) bits.push("known")
                                            if (n.connected) bits.push("joined")
                                            return bits.join(" · ")
                                        }
                                        font.family: nstele.faceMono
                                        font.pixelSize: 10
                                        color: nstele.withA(root.livery.holoBlue, 0.8)
                                        elide: Text.ElideRight
                                        width: Math.max(0, parent.width - nrMark.width
                                                        - 24 - nrName.width - 40)
                                    }
                                }

                                MouseArea {
                                    id: nrMa
                                    anchors.left: parent.left; anchors.right: parent.right
                                    anchors.top: parent.top
                                    height: 30
                                    hoverEnabled: true
                                    acceptedButtons: Qt.LeftButton | Qt.RightButton
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: function(mouse) {
                                        var n = netRow.modelData
                                        if (!n) return
                                        if (mouse.button === Qt.RightButton) {
                                            if (n.known) n.forget()
                                            return
                                        }
                                        nstele.lastFail = ""
                                        if (netRow.asking) { nstele.clearPsk(); return }
                                        if (n.connected) { n.disconnect(); return }
                                        if (!n.known && nstele.secured(n.security)) {
                                            nstele.openPsk(n)
                                            return
                                        }
                                        n.connect()
                                    }
                                }

                                // ── the PSK line — unrolls under a secured
                                // unknown row. Enter joins, Esc folds it.
                                Rectangle {
                                    visible: netRow.asking
                                    anchors.left: parent.left; anchors.right: parent.right
                                    anchors.leftMargin: 22; anchors.rightMargin: 4
                                    anchors.top: nrLine.bottom; anchors.topMargin: 4
                                    height: 24
                                    radius: 0
                                    color: "transparent"
                                    border.color: nstele.withA(nstele.sig, 0.8)
                                    border.width: 1
                                    TextInput {
                                        id: pskInput
                                        anchors.fill: parent
                                        anchors.leftMargin: 8; anchors.rightMargin: 64
                                        verticalAlignment: TextInput.AlignVCenter
                                        echoMode: TextInput.Password
                                        font.family: nstele.faceMono
                                        font.pixelSize: 11
                                        color: nstele.ink
                                        clip: true
                                        onAccepted: {
                                            var n = nstele.pskTarget
                                            if (n && text.length > 0) {
                                                n.connectWithPsk(text)
                                                text = ""
                                                nstele.clearPsk()
                                            }
                                        }
                                        Keys.onEscapePressed: {
                                            text = ""
                                            nstele.clearPsk()
                                        }
                                        // focus the line the moment it unrolls
                                        Connections {
                                            target: netRow
                                            function onAskingChanged() {
                                                if (netRow.asking) pskInput.forceActiveFocus()
                                                else pskInput.text = ""
                                            }
                                        }
                                    }
                                    Text {
                                        anchors.right: parent.right; anchors.rightMargin: 6
                                        anchors.verticalCenter: parent.verticalCenter
                                        text: "⏎ join"
                                        font.family: nstele.faceMono; font.pixelSize: 9
                                        color: nstele.withA(nstele.ink, 0.5)
                                    }
                                }
                            }
                        }
                    }

                    // one terracotta line when a join fails — cleared on retry
                    Text {
                        visible: nstele.lastFail !== ""
                        width: parent.width
                        text: nstele.lastFail
                        font.family: nstele.faceMono; font.pixelSize: 10
                        color: root.livery.paletteUrgent
                        elide: Text.ElideRight
                    }

                    // ── ledger: kaomoji + the interaction hint ──────────────
                    Item {
                        width: parent.width; height: 16
                        Rectangle {
                            anchors.top: parent.top
                            anchors.left: parent.left; anchors.right: parent.right
                            height: 1; color: nstele.withA(nstele.sig, 0.3)
                        }
                        Text {
                            anchors.left: parent.left; anchors.bottom: parent.bottom
                            text: nstele.kaomojiFor()
                            font.pixelSize: 11
                            color: nstele.withA(nstele.sig, 0.9)
                        }
                        Text {
                            anchors.right: parent.right; anchors.bottom: parent.bottom
                            text: "click · join   right · forget"
                            font.family: nstele.faceMono; font.pixelSize: 9
                            color: nstele.withA(nstele.ink, 0.5)
                        }
                    }

                    // ── box-drawing bottom frame — live figures, closing 𝄂 ──
                    Item {
                        width: parent.width; height: 18
                        Text {
                            id: netFfL
                            anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter
                            text: "└─┤ " + nstele.wifiSorted.length + " heard · "
                                  + nstele.connWord() + " ├"
                            font.family: nstele.faceMono; font.pixelSize: 11
                            color: nstele.withA(nstele.ink, 0.8)
                        }
                        Text {
                            id: netFfCorner
                            anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                            text: "┘"; font.family: nstele.faceMono; font.pixelSize: 11
                            color: nstele.withA(nstele.sig, 0.95)
                        }
                        Text {
                            id: netFfBar
                            anchors.right: netFfCorner.left; anchors.rightMargin: 4
                            anchors.verticalCenter: parent.verticalCenter
                            text: "𝄂"; font.family: nstele.faceMusic; font.pixelSize: 16
                            color: root.livery.paletteAccent
                        }
                        Rectangle {
                            anchors.left: netFfL.right; anchors.right: netFfBar.left
                            anchors.leftMargin: 2; anchors.rightMargin: 6
                            anchors.verticalCenter: parent.verticalCenter
                            anchors.verticalCenterOffset: 1
                            height: 1; color: nstele.withA(nstele.sig, 0.55)
                        }
                    }
                }
            }
        }
    }

    // Calendar — a per-song flavor-widget slot (CONTRACTS.md §5). No shared
    // fallback: the old song-blind CalendarGadget was retired, so a slot
    // NEITHER the active song NOR the baseline (sonata) provide simply
    // doesn't open (gated below on `resolveSong(...) !== ""`, not just
    // calShown) rather than popping an empty frame. Gating on the resolved
    // baseline chain — not the raw `has(activeSong, ...)` check this used
    // before the widget-slot expansion — matters now: a song that doesn't
    // author its own calendar.qml still gets sonata's via the WidgetSlot
    // below, so the popout must open for that case too, not just when the
    // active song directly authors it. Hosted BARE via StelePopout (khoa's
    // 2026-07-31 standing
    // direction, AudioColonnade precedent): the sonata calendar is now a
    // self-framed papyrus stele drawing all its own chrome — wrapping it in
    // BarPopout's GadgetFrame bay double-framed it (the Π order-mark goes
    // dormant with the bay, like Α Β Γ Δ Θ Ω before it). The widget carries
    // its own unfurl reveal off Window.visible; BarPopout's `reveal` seam
    // stays behind for any frame-hosted popout that wants it.
    // Hosted on SteleLayerPopout — the ONE bar popout on its own layer
    // surface (namespace "aoide-calendar") instead of an xdg_popup of
    // aoide-bar (khoa, 2026-08-13): the papyrus sheet cuts a transparent
    // window over its day grid that must show the desktop CRISPLY, and
    // aoide-bar's blur_popups layerrule frosts every xdg_popup with no
    // per-popup opt-out (popups carry no namespace). The other popouts stay
    // StelePopout/xdg_popup with their frost. Left-pinning comes free — the
    // layer surface is anchored top-left and grows rightward on the
    // compact↔expanded morph, the same twitch-free hang the old
    // anchorEdges/anchorGravity override bought (see SteleLayerPopout.qml).
    SteleLayerPopout {
        cell: clockText
        shown: root.calShown && root.stagingEngine.resolveSong(root.livery.songName, "calendar") !== ""
        WidgetSlot {
            livery: root.livery
            bridge: root.bridge
            stagingEngine: root.stagingEngine
            slot: "calendar"
        }
    }
}
