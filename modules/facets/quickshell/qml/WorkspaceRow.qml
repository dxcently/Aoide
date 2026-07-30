// WorkspaceRow.qml — the melody: workspaces as SOLID NOTE GLYPHS on the staff.
//
// Clean-slate redesign in the SONG vein (the entablature colonnade is gone). The
// bar is one measure of music; the workspaces are the notes written on it. Each
// live Hyprland workspace is now a SOLID (filled) musical note — not the old hand-
// drawn oval that rested hollow. Every workspace number maps 1:1 to a DISTINCT
// filled note glyph, so the row reads as recognisable notation and each number is
// identifiable by its shape + its pitch on the staff:
//
//     ws 1 → ♩   ws 2 → ♪   ws 3 → ♫   ws 4 → ♬   ws 5 → 𝅘𝅥𝅮   ws 6 → 𝅘𝅥𝅯
//     (ids past the set repeat the run; magic → 𝅘𝅥𝅱, scratch → 𝄋 ride the ledger)
//
// Pitch (staff degree) still rises with id so the open workspaces spell an
// ascending run. The SELECTED workspace is HIGHLIGHTED: its note swells, fills
// with the song's paletteAccent, and rests on a soft accent highlight-pill (the
// old black playhead re-cast as a glow behind the played note). Resting notes
// each carry their OWN hue — id → notes.noteColor(id) cycles the 8-slot base16
// accent spread (terracotta/base09/gold/laurel/teal/aegean/murex/rust), so every
// workspace is colour-identifiable at rest, not just by glyph shape. Urgent
// workspaces PULSE in glitchPink, overriding their resting hue.
// Clicking a note activates its workspace (Hyprland activate() — same real-service
// idiom, no shell-out, no invented IPC). Degrade: off Hyprland → an empty staff.
//
// Colour comes ONLY from the notes singleton, save one sanctioned literal:
// the white outline on the selected (active) glyph, for separation.

import QtQuick
import Quickshell.Hyprland

Item {
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
    // (drachma.noteColor cycles the 8-hue accent spread by id). Active/urgent/
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
        radius: 0
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
        radius: 0
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
                    radius: 0
                    color: root.notes.paletteAccent
                    opacity: 0.18
                }

                // The SOLID note glyph, parked at this note's pitch on the staff.
                // Resting → its own base16 accent hue (root.wsColor, one of 8
                // distinct colours cycled by workspace id — see DrachmaState.
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
