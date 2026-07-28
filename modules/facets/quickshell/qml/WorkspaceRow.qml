// WorkspaceRow.qml — the melody: workspaces as NOTE-HEADS on the staff.
//
// Clean-slate redesign in the SONG vein (the entablature colonnade is gone). The
// bar is one measure of music; the workspaces are the notes written on it. Each
// live Hyprland workspace is a note-head — a tilted oval sitting at its own pitch
// on the shared staff line that runs through the whole bar. The workspace id is
// the note's degree: id 1 sits low, ascending ids climb the staff, so the row of
// open workspaces reads as a little rising figure. The FOCUSED workspace is the
// note being played — a filled note-head with a stem, and a PLAYHEAD (the DAW
// cursor) eases across the staff to sit under it. Urgent workspaces PULSE
// (the blink_red homage, glitchPink). Clicking a note-head activates it.
//
// Binds to live Hyprland workspaces via Quickshell.Hyprland (same real-service
// idiom as before — no shell-out, no invented IPC). Degrade: off Hyprland → an
// empty staff. Colour follows the WHITE-SHEET re-theme: the note-heads, stems and
// playhead are BLACK ink (#000000 / #14141a) with a white legibility outline; the
// one state accent is the ACTIVE note-head, filled with the song's paletteAccent,
// and urgent workspaces keep glitchPink. The glyphs are content.

import QtQuick
import Quickshell.Hyprland

Item {
    id: root
    required property var notes

    // ── Geometry: note-heads on a five-line staff ──────────────────────────
    readonly property int cellW: 24        // horizontal slot per note
    readonly property int cellGap: 3
    readonly property real halfStep: 1.75  // line→space; a full staff line is 2×
    readonly property int headW: 17
    readonly property int headH: 12

    implicitWidth: cellRow.implicitWidth
    implicitHeight: 30

    // ── Note label + pitch ─────────────────────────────────────────────────
    // Regular workspaces carry their id; the two special workspaces keep a
    // single musical mark that still fits inside a note-head. Pitch (staff
    // degree) rises with id so the open workspaces spell an ascending run;
    // specials perch above the staff as a ledger note.
    function wsLabel(ws) {
        if (!ws) return "?"
        var name = ("" + (ws.name || "")).toLowerCase()
        if (name.indexOf("magic") !== -1) return "♬"
        if (name.indexOf("scratch") !== -1) return "ᝰ"
        return "" + ws.id
    }
    function wsSpecial(ws) {
        var name = ws ? ("" + (ws.name || "")).toLowerCase() : ""
        return name.indexOf("magic") !== -1 || name.indexOf("scratch") !== -1
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

    // Index of the focused note (drives the playhead); -1 when none.
    readonly property int activeIndex: {
        var vs = root.wsList
        for (var i = 0; i < vs.length; i++)
            if (vs[i] && (vs[i].active || vs[i].focused)) return i
        return -1
    }

    // ── The playhead — a soft cursor that eases to the note being played ────
    // Declared before the notes so the heads sit ON it. Spans the staff band.
    Rectangle {
        id: playhead
        visible: root.activeIndex >= 0
        z: 0
        width: 2
        height: 22
        radius: 1
        color: "#000000"
        opacity: 0.3
        anchors.verticalCenter: parent.verticalCenter
        x: root.activeIndex >= 0
           ? root.activeIndex * (root.cellW + root.cellGap) + root.cellW / 2 - width / 2
           : 0
        Behavior on x {
            NumberAnimation { duration: 180; easing.type: Easing.OutCubic }
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

                width: root.cellW
                height: root.height

                // The note-head group: a tilted oval + its upright number,
                // parked at this note's pitch on the staff.
                Item {
                    id: noteGroup
                    width: root.headW
                    height: root.headH
                    x: (cell.width - width) / 2
                    y: (cell.height - height) / 2 + root.pitchOffset(cell.modelData)

                    // Stem — the quarter-note upstroke, only on the played note.
                    Rectangle {
                        id: stem
                        visible: cell.isActive
                        width: 1.5
                        height: 13
                        radius: 0.5
                        color: "#000000"
                        x: parent.width - 1.5
                        y: -12
                    }

                    // Note-head — the ACTIVE note fills with the song's accent
                    // (the one played note); resting heads are hollow black ovals;
                    // an urgent workspace outlines in glitchPink. The tilt is the
                    // engraver's slanted oval.
                    Rectangle {
                        id: head
                        anchors.fill: parent
                        radius: height / 2
                        rotation: -20
                        color: cell.isActive ? root.notes.paletteAccent : "transparent"
                        border.width: cell.isActive ? 0 : 1.5
                        border.color: cell.isUrgent ? root.notes.glitchPink
                                                    : "#000000"
                        Behavior on color { ColorAnimation { duration: 150 } }
                    }

                    // The degree number / special mark, upright over the head.
                    // Black ink at rest; white on the accent-filled played note.
                    Text {
                        anchors.centerIn: parent
                        text: root.wsLabel(cell.modelData)
                        color: cell.isActive ? "#ffffff"
                              : (cell.isUrgent ? root.notes.glitchPink : "#14141a")
                        style: Text.Outline
                        styleColor: cell.isActive ? root.notes.paletteAccent : "#ffffff"
                        font.family: "monospace"
                        font.pixelSize: 11
                        font.bold: cell.isActive
                    }
                }

                // Urgent pulse (blink_red homage) — breathes the whole note.
                SequentialAnimation on opacity {
                    running: cell.isUrgent
                    loops: Animation.Infinite
                    NumberAnimation { to: 0.35; duration: 600; easing.type: Easing.InOutQuad }
                    NumberAnimation { to: 1.0;  duration: 600; easing.type: Easing.InOutQuad }
                }

                MouseArea {
                    anchors.fill: parent
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
