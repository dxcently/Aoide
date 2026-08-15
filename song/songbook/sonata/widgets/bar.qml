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
// `notes`/`bridge`: `stagingEngine` (so its own embedded calendar
// `WidgetSlot` can resolve), `powermenu` (the powermenu slot's live
// `.item`, for the clef), `shared` (session state, optional).
// `WorkspaceRow.qml` travels alongside this file as a helper component
// (uppercase filename — carried by the build, never independently
// resolvable as a slot). `StelePopout`/`BarPopout`/`SteleLayerPopout`/
// `WidgetSlot`/`AudioColonnade` stay in the facet as shared, reusable
// popout-hosting chrome (not bar-specific content) — reached via the
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
//               live, with popouts. Closed by a final barline 𝄂.
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
// manuscript paper, no glass and no gloss. The page fill is notes.paletteBg
// at full alpha (Qt.rgba(paletteBg.r, .g, .b, 1.0)); there is no blur behind
// it and no gradient sheen on top — the strip reads as solid paper. The
// structural ink stays BLACK (#000000 staff lines, barlines, playhead), but
// the TEXT ink is now the song's umber (notes.paletteFg #423420) drawn CLEAN
// — the old white legibility outline is dropped, since dark text on the
// opaque sheet needs no halo (that outline was a relic of the old dark bar
// and only muddied the type on cream).
// One restrained STATE accent survives from the song (LiveryState): the ACTIVE
// workspace note-head fills with paletteAccent, the BLOCKED ✎ pulse + low battery
// go glitchPink, and open/hover toggles (volume) flash paletteAccent.
// Everything at rest is black.
//
// Data sources (unchanged real Quickshell services — the plumbing survives):
//   - Hyprland   → workspaces (WorkspaceRow) + active window title.
//   - Pipewire   → default sink volume / muted.
//   - UPower     → display-device battery.
//   - Network    → /proc/net/route via FileView (files-not-processes rule).
//   - Sessions   → song/stage/sessions.json + hooks.json via FileView.

import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import Quickshell.Hyprland
import Quickshell.Services.Pipewire
import Quickshell.Services.UPower
// Reaches the facet's shared, reusable popout-hosting chrome (WidgetSlot,
// StelePopout, BarPopout, SteleLayerPopout, AudioColonnade) — none of it is
// bar-specific content, so it stays in the facet rather than moving here.
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
// one measure of music; the workspaces are the notes written on it. Each
// live Hyprland workspace is a SOLID (filled) musical note, one distinct
// glyph per workspace id:
//     ws 1 → ♩   ws 2 → ♪   ws 3 → ♫   ws 4 → ♬   ws 5 → 𝅘𝅥𝅮   ws 6 → 𝅘𝅥𝅯
//     (ids past the set repeat the run; magic → 𝅘𝅥𝅱, scratch → 𝄋 ride the ledger)
// Pitch (staff degree) rises with id. The SELECTED workspace swells, fills
// with its own hue, and rests on a soft accent highlight-pill. Resting notes
// each carry their own hue (notes.noteColor cycles the 8-slot base16 accent
// spread); urgent workspaces PULSE in glitchPink, overriding the resting hue.
// Clicking a note activates its workspace (Hyprland activate(), no shell-out).
// Colour comes ONLY from the notes singleton, save one sanctioned literal:
// the white outline on the selected glyph, for separation.
component WorkspaceRow: Item {
    id: root
    required property var notes

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

    // ── Geometry: solid notes on a five-line staff ─────────────────────────
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
    // (notes.noteColor cycles the 8-hue accent spread by id). Active/urgent/
    // preview states still override this in the delegate below.
    function wsColor(ws) {
        var id = ws ? (ws.id || 1) : 1
        return root.notes.noteColor(id)
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
        ? root.notes.noteColor(root.wsList[root.activeIndex].id)
        : root.notes.paletteAccent

    // ── The highlight-pill — a soft accent glow that eases under the played
    // note (the old playhead re-cast). Declared before the notes so it sits
    // behind them, reinforcing which workspace is selected.
    Rectangle {
        id: highlight
        visible: root.activeIndex >= 0
        z: 0
        width: root.cellW - 2
        height: 22
        // Fully rounded on the short axis — an actual pill, which is what the
        // name promised and the square-cornered block never read as.
        radius: width / 2
        color: root.activeColor
        opacity: 0.20
        anchors.verticalCenter: parent.verticalCenter
        x: root.activeIndex >= 0
           ? root.activeIndex * (root.cellW + root.cellGap) + (root.cellW - width) / 2
           : 0
        Behavior on x {
            NumberAnimation { duration: 180; easing.type: Easing.OutCubic }
        }
    }

    // ── The preview RING — the hover-preview highlight (distinct from active).
    // A hollow holoBlue outline that eases under the workspace the hovered
    // terminal lives on. Deliberately a DIFFERENT KIND of mark from the active
    // pill (hollow cool ring vs solid warm accent fill), so both can show at
    // once — if the previewed ws IS the active one, the ring simply frames the
    // accent pill and reads sensibly. Border-only, input-inert. Colour: notes.
    Rectangle {
        id: previewRing
        visible: root.hoveredIndex >= 0
        z: 0
        width: root.cellW
        height: 24
        radius: width / 2      // matches the active pill's curve, one size out
        color: "transparent"
        border.color: root.notes.paletteAccent
        border.width: 2
        opacity: 0.85
        anchors.verticalCenter: parent.verticalCenter
        x: root.hoveredIndex >= 0
           ? root.hoveredIndex * (root.cellW + root.cellGap) + (root.cellW - width) / 2
           : 0
        Behavior on x {
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
                    width: root.cellW - 2
                    height: 22
                    radius: width / 2   // same pill as the active/preview marks
                    color: root.notes.paletteAccent
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
                          : (cell.isUrgent ? root.notes.glitchPink
                                           : (cell.isHovered ? root.notes.paletteAccent
                                                             : (cell.isPreview ? root.notes.paletteAccent
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
    required property var notes
    required property var bridge
    // The staging engine (StagingEngine singleton, injected as an extra —
    // it isn't part of WidgetSlot's universal notes/bridge contract) — the
    // calendar popout below asks it whether the active song (or, through
    // the baseline chain, sonata) dresses the "calendar" slot before
    // opening, and the popout's own embedded WidgetSlot needs the same
    // engine handle to resolve.
    required property var stagingEngine
    // The Exodos powermenu (AoideExodos instance, injected by shell.qml the
    // same way the launcher receives the clipboard). The clef toggles it
    // directly — the old `{ cmd: "powermenu" }` bridge line was a dead end
    // (shellbridge never parsed that verb).
    required property var powermenu
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
    // Click → open the gadget dock (bridge dock verb, the AoideAgentWidgets path).
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
    readonly property string sessionsPath:
        Quickshell.env("HOME") + "/Aoide/song/stage/sessions.json"
    readonly property string hooksPath:
        Quickshell.env("HOME") + "/Aoide/song/stage/hooks.json"
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

    // ── Network (procfs FileView — no NM service at this rev) ───────────────
    // /proc/net/route lists every iface that has a route; the default route is
    // the row whose Destination field is "00000000". Reading a kernel file (not
    // spawning a process) keeps us inside the no-shell-out rule. Real iface
    // names — classified wifi vs ethernet by prefix (wl* → wifi).
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
    Component.onCompleted: rollKaomoji()
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
    // The audio popout (the two-column colonnade) is shared: it opens while the
    // pointer is over the ♪ vol cell, the ● mic cell, OR the popout body itself
    // (so its columns can be clicked/scrolled without it closing under you).
    property bool volCellHover: false  // pointer over the vol cell
    property bool micCellHover: false  // pointer over the mic cell
    property bool audioBodyHover: false // pointer inside the colonnade popout
    readonly property bool audioShown: volCellHover || micCellHover || audioBodyHover
    property bool battShown: false     // battery hover popout
    property bool calShown: false      // calendar click popout (song widget slot)

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
        color: "#000000"
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
        color: Qt.rgba(Qt.color(root.notes.paletteBg).r,
                       Qt.color(root.notes.paletteBg).g,
                       Qt.color(root.notes.paletteBg).b, 1.0)
        opacity: 1.0
    }
    // The page rail — a DEFINED black edge framing the opaque strip.
    Rectangle {
        anchors.fill: page
        radius: 0
        color: "transparent"
        border.color: "#000000"
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
            color: "#000000"
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
        color: root.notes.paletteFg
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
            // parser never knew that verb.)
            onClicked: if (root.powermenu) root.powermenu.toggle()
        }
    }

    // ══ LEFT STAVE — sessions · gadget tray · clock / title ════════════════
    // Written just after the clef, reading left to right like the opening of
    // the measure. Everything centres on the staff.
    Row {
        id: leftContent
        anchors.left: clefText.right
        anchors.leftMargin: 12
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
            color: root.anyBlocked ? root.notes.glitchPink : root.notes.paletteFg
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
                onClicked: root.bridge.sendCommand({ cmd: "dock", action: "toggle" })
            }
        }

        // ── Clock + date. Lives on the bar, not the dock — the ambient face
        // belongs beside the measure, not in the case.
        Text {
            id: clockText
            anchors.verticalCenter: parent.verticalCenter
            text: Qt.formatDateTime(root.now, "hh:mm AP  dddd MMM dd")
            color: root.calShown ? root.notes.paletteAccent : root.notes.paletteFg
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
            anchors.verticalCenter: parent.verticalCenter
            text: "/"
            color: root.notes.paletteFg
            font.family: "monospace"
            font.pixelSize: 14
            opacity: 0.55
        }
        // Active-window title (music kaomoji when empty). khoa, 2026-07-31:
        // primary-color ink — paletteAccent, not paletteFg.
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: root.winTitle()
            color: root.notes.paletteAccent
            font.family: "monospace"
            font.pixelSize: 14
            font.bold: true
            elide: Text.ElideRight
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
            notes: root.notes
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

        // Volume (OUTPUT) — scroll = adjust, click = mute, hover = the colonnade
        // popout (shared with mic). Attic-gold ink; hover feedback is
        // opacity + underline (the base is already the accent).
        Text {
            id: volText
            anchors.verticalCenter: parent.verticalCenter
            visible: root.volAvail
            text: root.volMuted ? "𝄽 vol" : (root.volIcon(root.volPct) + " " + root.volPct)
            color: root.notes.paletteAccent
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
                onClicked: root.volToggleMute()
                onWheel: {
                    root.volAdjust(wheel.angleDelta.y > 0 ? 2 : -2)
                    wheel.accepted = true
                }
            }
        }

        // Microphone (INPUT) — the cool aegean voice, paired beside the vol cell.
        // scroll = adjust capture gain, click = mute, hover = the shared popout.
        // ● recording dot when live; a 𝄽 rest when muted (mirrors the vol cell).
        Text {
            id: micText
            anchors.verticalCenter: parent.verticalCenter
            visible: root.micAvail
            text: root.micMuted ? "𝄽 mic" : ("● " + root.micPct)
            color: root.notes.holoBlue
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
                onClicked: root.micToggleMute()
                onWheel: {
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
            color: (root.battCrit || root.battWarn) ? root.notes.glitchPink
                                                    : root.notes.paletteFg
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
        // to a dim rest (𝄽) at reduced opacity.
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: root.netGlyph(root.netKind) + " " + root.netLabel(root.netKind)
            color: root.notes.paletteFg
            opacity: root.netKind === "down" ? 0.5 : 1.0
            font.family: "monospace"
            font.pixelSize: 14
        }

        // The final barline — thin + thick black rules, closing the measure (𝄂).
        Row {
            anchors.verticalCenter: parent.verticalCenter
            spacing: 2
            Rectangle {
                width: 1.5; height: root.staffSpan + 4; radius: 0.5
                color: "#000000"; opacity: 0.7
                anchors.verticalCenter: parent.verticalCenter
            }
            Rectangle {
                width: 3; height: root.staffSpan + 4; radius: 0.5
                color: "#000000"; opacity: 0.9
                anchors.verticalCenter: parent.verticalCenter
            }
        }
    }

    // ══ POPOUTS — real PopupWindows under their bar cells (BarPopout) ══════
    // Each is its own xdg_popup with GadgetFrame chrome (glass via blur_popups).
    // The cell ids above (volText/battText) anchor them. Meters/power/clock
    // popouts moved to the AoideAgentWidgets dock (bottom-seated frames).

    // Audio control — the two-column colonnade (MIC + VOL), a self-framed marble
    // stele hosted BARE (StelePopout, no GadgetFrame — it draws its own chrome).
    // Shared popout hung under the vol cell, opened by hovering EITHER audio cell
    // (or the body, so its columns can be clicked/scrolled without it closing).
    StelePopout {
        cell: volText
        shown: root.audioShown && (root.volAvail || root.micAvail)
        AudioColonnade {
            notes: root.notes
            outPct: root.volPct
            outMuted: root.volMuted
            outAvail: root.volAvail
            inPct: root.micPct
            inMuted: root.micMuted
            inAvail: root.micAvail
            onOutToggle: root.volToggleMute()
            onOutAdjust: function(d) { root.volAdjust(d) }
            onInToggle: root.micToggleMute()
            onInAdjust: function(d) { root.micAdjust(d) }
            onHoveredChanged: root.audioBodyHover = hovered
        }
    }

    // Battery hover popout — time-remaining + charge bar.
    BarPopout {
        notes: root.notes
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
                color: root.battWarn ? root.notes.paletteUrgent : "#14141a"
                font.family: "monospace"
                font.pixelSize: 14
            }
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                visible: root.battTime().length > 0
                text: root.battTime()
                color: "#14141a"
                opacity: 0.75
                font.family: "monospace"
                font.pixelSize: 11
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
        shown: root.calShown && root.stagingEngine.resolveSong(root.notes.songName, "calendar") !== ""
        WidgetSlot {
            notes: root.notes
            bridge: root.bridge
            stagingEngine: root.stagingEngine
            slot: "calendar"
        }
    }
}
