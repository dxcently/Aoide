// CalendarGadget.qml — ASCII month grid, for the gadget dock.
//
// The gadget-dock descendant of the dxflake waybar clock's calendar tooltip:
// the current month drawn in box-drawing chrome, today highlighted in the note
// accent colour. Self-contained (a QML Date + a per-minute Timer to roll over
// midnight; no service, no stage file). All colours from notes (zero hardcoded
// hex); the box-drawing chrome is content.
//
// Also usable as an anchored bar popup (the clock cell drops it under the bar):
// it is a plain Item sized to its content, so a parent Popup/Rectangle can host
// it directly.

import QtQuick

Item {
    id: root

    required property var notes

    // ── Live date (roll over at midnight; a minute tick is plenty) ─────────
    property var now: new Date()
    Timer {
        interval: 60000
        repeat: true
        running: true
        onTriggered: root.now = new Date()
    }

    readonly property int year: now.getFullYear()
    readonly property int month: now.getMonth()        // 0..11
    readonly property int today: now.getDate()
    readonly property var monthNames: [
        "January", "February", "March", "April", "May", "June",
        "July", "August", "September", "October", "November", "December"
    ]

    // ── Grid model: rows of 7 cells, each { day, isToday } (day 0 = blank) ──
    readonly property var weeks: {
        var first = new Date(year, month, 1)
        var startDow = first.getDay()                  // 0=Sun
        var daysInMonth = new Date(year, month + 1, 0).getDate()
        var cells = []
        for (var b = 0; b < startDow; b++) cells.push(0)
        for (var d = 1; d <= daysInMonth; d++) cells.push(d)
        while (cells.length % 7 !== 0) cells.push(0)
        var rows = []
        for (var i = 0; i < cells.length; i += 7)
            rows.push(cells.slice(i, i + 7))
        return rows
    }

    // ── ASCII chrome lines (21 interior cols: 7 days × "NN ") ──────────────
    readonly property int interior: 21
    function pad2(n) { return (n < 10 ? " " : "") + n }
    function centerTitle() {
        var t = monthNames[month] + " " + year
        var total = interior
        var left = Math.floor((total - t.length) / 2)
        if (left < 0) left = 0
        var s = ""
        for (var i = 0; i < left; i++) s += " "
        s += t
        while (s.length < total) s += " "
        return s
    }
    function rule(l, r) {
        var mid = ""
        for (var i = 0; i < interior; i++) mid += "─"
        return l + mid + r
    }

    implicitHeight: column.implicitHeight

    Column {
        id: column
        spacing: 0

        // ── Ornament top rule: short staff run above the grid (ornament
        // vocab). Standalone — the staff glyphs are wide SMP chars, so they
        // sit ABOVE the box rather than inside the alignment-exact ┌─┐ math.
        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            text: "𝄂𝄚𝅦𝄚"
            color: root.notes.paletteAccent
            opacity: 0.5
            font.family: "monospace"
            font.pixelSize: 11
        }

        // ── Top rule + title ─────────────────────────────────────────────
        Text {
            text: root.rule("┌", "┐")
            color: root.notes.wireCyan
            opacity: 0.5
            font.family: "monospace"
            font.pixelSize: 12
        }
        Row {
            Text { text: "│"; color: root.notes.wireCyan; opacity: 0.5; font.family: "monospace"; font.pixelSize: 12 }
            Text {
                text: root.centerTitle()
                color: root.notes.paletteFg
                font.family: "monospace"
                font.pixelSize: 12
                font.bold: true
            }
            Text { text: "│"; color: root.notes.wireCyan; opacity: 0.5; font.family: "monospace"; font.pixelSize: 12 }
        }
        Text {
            text: root.rule("├", "┤")
            color: root.notes.wireCyan
            opacity: 0.5
            font.family: "monospace"
            font.pixelSize: 12
        }

        // ── Weekday header ───────────────────────────────────────────────
        Row {
            Text { text: "│"; color: root.notes.wireCyan; opacity: 0.5; font.family: "monospace"; font.pixelSize: 12 }
            Text {
                text: "Su Mo Tu We Th Fr Sa"
                color: root.notes.paletteFg
                opacity: 0.7
                font.family: "monospace"
                font.pixelSize: 12
            }
            Text { text: "│"; color: root.notes.wireCyan; opacity: 0.5; font.family: "monospace"; font.pixelSize: 12 }
        }

        // ── Week rows ────────────────────────────────────────────────────
        Repeater {
            model: root.weeks
            delegate: Row {
                id: weekRow
                required property var modelData
                Text { text: "│"; color: root.notes.wireCyan; opacity: 0.5; font.family: "monospace"; font.pixelSize: 12 }
                Row {
                    spacing: 0
                    Repeater {
                        model: weekRow.modelData
                        delegate: Text {
                            required property var modelData
                            readonly property bool isToday:
                                modelData === root.today
                            // "NN " per cell; blank cells are spaces.
                            text: (modelData === 0 ? "   "
                                   : (root.pad2(modelData) + " "))
                            color: isToday ? root.notes.paletteAccent : root.notes.paletteFg
                            font.family: "monospace"
                            font.pixelSize: 12
                            font.bold: isToday
                        }
                    }
                }
                Text { text: "│"; color: root.notes.wireCyan; opacity: 0.5; font.family: "monospace"; font.pixelSize: 12 }
            }
        }

        // ── Bottom rule ──────────────────────────────────────────────────
        Text {
            text: root.rule("└", "┘")
            color: root.notes.wireCyan
            opacity: 0.5
            font.family: "monospace"
            font.pixelSize: 12
        }
    }
}
