// WorkspaceRow.qml — musical-notation workspace switcher (dxflake homage).
//
// Binds to live Hyprland workspaces via Quickshell.Hyprland (the same
// real-service idiom AoideNotifications uses for NotificationServer — no
// shell-out, no invented IPC). Each workspace renders as a small square cell
// with a musical-notation glyph (the dxflake waybar's format-icons map).
//
// Quickshell-native enhancement over static waybar: the ACTIVE-cell box is a
// SINGLE overlay Rectangle that EASES (Behavior on x/width) between cells as the
// focus moves, instead of a hard per-cell border toggle. Urgent workspaces PULSE
// (opacity animation — the CSS blink_red homage). Clicking a cell activates it.
//
// All colours from notes (zero hardcoded hex). The glyph STRINGS are content
// (musical notation), not palette. The black text-outline is the one legibility
// literal, matching the rig.  Degrade: off Hyprland (no workspaces) → empty row.

import QtQuick
import Quickshell.Hyprland

Item {
    id: root
    required property var notes

    readonly property int cellSize: 28
    readonly property int cellGap: 4

    implicitWidth: cellRow.implicitWidth
    implicitHeight: cellSize

    // ── Musical format-icons (dxflake waybar) ──────────────────────────────
    function wsIcon(ws) {
        if (!ws) return "?"
        var name = ("" + (ws.name || "")).toLowerCase()
        if (name.indexOf("magic") !== -1) return "♬⋆.˚"
        if (name.indexOf("scratch") !== -1) return "ᝰ.ᐟ"
        var byId = {
            "1": "𝅘𝅥", "2": "♫", "3": "𝅘𝅥𝅯", "4": "♬", "5": "𝅘𝅥𝅱",
            "6": "𝅗𝅥", "7": "𝅝", "8": "♯", "9": "♮", "10": "♭"
        }
        var g = byId["" + ws.id]
        return g !== undefined ? g : ("" + ws.id)
    }

    // Sorted live workspaces (by id) — ObjectModel exposes `.values`.
    readonly property var wsList: {
        var vs = (Hyprland.workspaces && Hyprland.workspaces.values)
                 ? Hyprland.workspaces.values.slice() : []
        vs.sort(function (a, b) { return (a.id || 0) - (b.id || 0) })
        return vs
    }

    // Index of the active cell (for the sliding box); -1 when none.
    readonly property int activeIndex: {
        var vs = root.wsList
        for (var i = 0; i < vs.length; i++)
            if (vs[i] && (vs[i].active || vs[i].focused)) return i
        return -1
    }

    Row {
        id: cellRow
        spacing: root.cellGap

        Repeater {
            model: root.wsList
            delegate: Item {
                id: cell
                required property var modelData
                required property int index
                readonly property bool isActive: root.activeIndex === cell.index
                readonly property bool isUrgent: cell.modelData && cell.modelData.urgent

                width: root.cellSize
                height: root.cellSize

                Text {
                    anchors.centerIn: parent
                    text: root.wsIcon(cell.modelData)
                    color: cell.isActive ? root.notes.barAccent
                          : (cell.isUrgent ? root.notes.paletteUrgent : root.notes.barFg)
                    style: Text.Outline
                    styleColor: "#000000"
                    font.family: "monospace"
                    font.pixelSize: 17
                    font.bold: cell.isActive

                    // Urgent pulse (blink_red homage).
                    SequentialAnimation on opacity {
                        running: cell.isUrgent
                        loops: Animation.Infinite
                        NumberAnimation { to: 0.35; duration: 600; easing.type: Easing.InOutQuad }
                        NumberAnimation { to: 1.0;  duration: 600; easing.type: Easing.InOutQuad }
                    }
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

    // ── Sliding active-box overlay (eases between cells) ──────────────────
    // A SIBLING of the Row, not a child: a Row positioner assigns x to every
    // child, so an in-Row overlay gets slotted as item #0 (an empty square
    // beside the cells — the v2 rendering bug) instead of floating over them.
    Rectangle {
        id: activeBox
        visible: root.activeIndex >= 0
        z: 1
        width: root.cellSize
        height: root.cellSize
        radius: 0
        color: "transparent"
        border.color: root.notes.barAccent
        border.width: 1
        x: root.activeIndex >= 0
           ? root.activeIndex * (root.cellSize + root.cellGap) : 0
        y: 0
        Behavior on x {
            NumberAnimation { duration: 150; easing.type: Easing.OutCubic }
        }
    }
}
