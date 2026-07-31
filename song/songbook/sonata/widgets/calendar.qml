// calendar.qml — sonata/Greek-marble "calendar" flavor widget: the COMPOSITE
// temple, the invented SIXTH order.
//
// khoa, 2026-07-31: full rewrite of the proof-stub month grid into the real
// "fuller calendar, scroll-open" pass. Joins the dock's four temples
// (song/songbook/sonata/design/greek-grammar.md §4) as a fifth surface with
// its own order, the same way NotificationCard invented Tuscan for HERALD:
//
//   · ORDER    — COMPOSITE, the sixth classical order (a Corinthian capital's
//                acanthus fused with an Ionic volute) — fitting for a
//                calendar, which fuses two readings itself (a grid AND a
//                date). None of the four dock temples or HERALD's Tuscan
//                claims it.
//   · SIGNATURE — `notes.base09`, kiln-fired clay orange — sonata's own
//                 unclaimed accentSpread slot (DrachmaState.qml's comment
//                 already calls it "unnamed elsewhere", same slot HERALD's
//                 Tuscan sibling base0F sits beside). No DrachmaState change
//                 needed — base09 already resolves with a paletteAccent
//                 fallback.
//   · CROWN    — 𝄴 (U+1D134, common-time signature) stands in for a clef,
//                the same "no clef fits" move as Power's ϟ and HERALD's ❧ —
//                a calendar keeps time, not pitch.
//   · FRIEZE   — a Canvas-drawn Vitruvian wave-scroll (running-dog): a band
//                of connected spiral curls, genuinely distinct from the
//                dock's meander / volute+dentil+egg-and-dart / triglyph-
//                metope / egg-and-dart-ovolo friezes.
//   · LAUREL   — today's cell only (`paletteHot` fill, `paletteBg` numeral);
//                no laurel while browsing another month (`monthOffset !== 0`).
//
// A librarian pass should add this Composite row to greek-grammar.md §4's
// table later (out of scope here — same interim-record precedent
// NotificationCard's own header sets for Tuscan).
//
// ── The "fuller calendar" mechanic ──────────────────────────────────────
// A FIXED 42-cell (6×7) grid, wheel-paged by month (`monthOffset`, no
// ListView/PathView) — always 6 rows, so the popup height never changes
// across months (no ragged corners, no resize). Leading/trailing cells
// outside the viewed month show the adjacent month's days at 0.3 opacity.
// `implicitHeight` is a FIXED constant (summed below), not `childrenRect`,
// because BarPopout sizes its window off this and month-paging must not
// jiggle the popup.
//
// Fixed injected-prop contract: `notes` + `bridge` (bridge unused — declared
// per the contract, same as before).

import QtQuick

Item {
    id: root

    required property var notes
    required property var bridge

    readonly property string faceSerif: "Noto Serif"
    readonly property string faceMono:  "JetBrainsMono Nerd Font"
    readonly property string faceMusic: "Noto Music"
    readonly property color clay: notes.base09

    function withA(cstr, a) {
        var c = Qt.color(cstr)
        return Qt.rgba(c.r, c.g, c.b, a)
    }

    // ── Fixed size — summed from the content below, NOT childrenRect (the
    // popup window must not resize as monthOffset pages or the reveal
    // animation plays) ──────────────────────────────────────────────────
    implicitWidth: 248
    implicitHeight: 229   // 15 top-frame + 20 header + 10 frieze + 14 weekday
                           // + 106 grid (6*16 + 5*2) + 16 tally + 18 bottom-frame
                           // + 6*5 column spacing = 229

    property date now: new Date()
    Timer {
        interval: 60000
        running: true
        repeat: true
        onTriggered: root.now = new Date()
    }

    // ── Month paging state — wheel over the grid, ‹/› click targets, or
    // click the month name to reset to today ───────────────────────────
    property int monthOffset: 0
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

    function kaomojiFor() {
        if (root.monthOffset === 0) return "( ´ ▽ ` )"
        if (root.monthOffset < 0) return "(￢_￢ )"
        return "(☆ ≧▽≦)"
    }

    Column {
        id: mainColumn
        width: parent.width
        spacing: 5

        // ── TUI top frame: ┌─┤ 𝄴 kalendae ├──────────┐ ────────────────
        Item {
            width: parent.width
            height: 15
            Row {
                id: tfL
                anchors.left: parent.left
                anchors.verticalCenter: parent.verticalCenter
                spacing: 0
                Text {
                    text: "┌─┤ "
                    font.family: root.faceMono; font.pixelSize: 11
                    color: root.clay
                }
                Text {
                    text: "𝄴"
                    font.family: root.faceMusic; font.pixelSize: 13
                    color: root.clay
                }
                Text {
                    text: " kalendae ├"
                    font.family: root.faceMono; font.pixelSize: 11
                    color: root.clay
                }
            }
            Text {
                id: tfR
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                text: "┐"
                font.family: root.faceMono; font.pixelSize: 11
                color: root.clay
            }
            Rectangle {
                anchors.left: tfL.right; anchors.right: tfR.left
                anchors.leftMargin: 2; anchors.rightMargin: 2
                anchors.verticalCenter: parent.verticalCenter
                height: 1; color: root.withA(root.clay, 0.55)
            }
        }

        // ── Header row: ‹ month year › — click month/year resets to today ──
        Item {
            width: parent.width
            height: 20

            Text {
                id: prevArrow
                anchors.left: parent.left
                anchors.verticalCenter: parent.verticalCenter
                text: "‹"
                color: root.clay
                font.family: root.faceSerif
                font.pixelSize: 16
                font.bold: true
                MouseArea {
                    anchors.fill: parent
                    anchors.margins: -4
                    cursorShape: Qt.PointingHandCursor
                    onClicked: root.monthOffset -= 1
                }
            }
            Row {
                id: monthYearRow
                anchors.centerIn: parent
                spacing: 5
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: root.monthNames[root.viewMonth]
                    color: root.notes.paletteFg
                    font.family: root.faceSerif
                    font.pixelSize: 14
                    font.bold: true
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: "" + root.viewYear
                    color: root.notes.paletteAccent
                    font.family: root.faceSerif
                    font.pixelSize: 14
                    font.bold: true
                }
            }
            // Click target sits OUTSIDE monthYearRow (a Row): a MouseArea
            // using anchors.fill would conflict with Row's own x-axis
            // management if nested inside it, so it's a sibling here instead,
            // anchored to the Row's bounds — this parent Item isn't a
            // positioner, so that's unrestricted.
            MouseArea {
                anchors.left: monthYearRow.left; anchors.right: monthYearRow.right
                anchors.top: monthYearRow.top; anchors.bottom: monthYearRow.bottom
                anchors.margins: -4
                cursorShape: Qt.PointingHandCursor
                onClicked: root.monthOffset = 0
            }
            Text {
                id: nextArrow
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                text: "›"
                color: root.clay
                font.family: root.faceSerif
                font.pixelSize: 16
                font.bold: true
                MouseArea {
                    anchors.fill: parent
                    anchors.margins: -4
                    cursorShape: Qt.PointingHandCursor
                    onClicked: root.monthOffset += 1
                }
            }
        }

        // ── Frieze — a Canvas-drawn Vitruvian wave-scroll (running-dog) ────
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
                    // one 1.5-turn spiral curl
                    ctx.beginPath()
                    var steps = 24
                    for (var s = 0; s <= steps; s++) {
                        var t = s / steps
                        var ang = t * Math.PI * 3          // 1.5 turns = 540deg
                        var r = maxR * (1 - t)
                        var x = cx + r * Math.cos(ang)
                        var y = midY + r * Math.sin(ang) * 0.6
                        if (s === 0) ctx.moveTo(x, y); else ctx.lineTo(x, y)
                    }
                    ctx.stroke()
                    // tangent connector to the next unit's curl
                    var outerX = cx + maxR
                    var nextCx = (i + 1) * unit + unit * 0.35
                    var nextOuterX = nextCx - maxR
                    ctx.beginPath()
                    ctx.moveTo(outerX, midY)
                    ctx.lineTo(nextOuterX, midY)
                    ctx.stroke()
                }
            }
        }

        // ── Weekday caps — Greek-initial letters, clay ink ──────────────────
        Row {
            anchors.horizontalCenter: parent.horizontalCenter
            height: 14
            spacing: 2
            Repeater {
                model: root.dayLetters
                delegate: Text {
                    required property var modelData
                    width: 24
                    horizontalAlignment: Text.AlignHCenter
                    text: modelData
                    color: root.clay
                    font.family: root.faceSerif
                    font.pixelSize: 12
                    font.bold: true
                }
            }
        }

        // ── The fixed 42-cell (6×7) grid — wheel over it pages months ──────
        // The wheel MouseArea is a SIBLING of the grid Column (not nested
        // inside it) — a Column positions every child it owns, so a
        // MouseArea child there would be laid out as a 7th "row" and wreck
        // both the fixed 6-row grid and the fixed implicitHeight math above.
        Item {
            id: gridWrap
            anchors.horizontalCenter: parent.horizontalCenter
            width: grid.width
            height: grid.height

            Column {
                id: grid
                spacing: 2

                Repeater {
                    model: 6
                    delegate: Row {
                        required property int index
                        readonly property int weekIdx: index
                        spacing: 2

                        Repeater {
                            model: 7
                            delegate: Rectangle {
                                required property int index
                                readonly property int cellIdx: parent.weekIdx * 7 + index
                                readonly property int dayOffset: cellIdx - root.firstWeekday + 1
                                readonly property bool isPrev: dayOffset < 1
                                readonly property bool isNext: dayOffset > root.daysInMonth
                                readonly property bool isCurrent: !isPrev && !isNext
                                readonly property int dayNum:
                                    isPrev ? (root.daysInPrevMonth + dayOffset)
                                           : (isNext ? (dayOffset - root.daysInMonth) : dayOffset)
                                readonly property bool isToday:
                                    root.monthOffset === 0 && isCurrent && dayNum === root.today

                                width: 24
                                height: 16
                                radius: 0
                                color: isToday ? root.notes.paletteHot : "transparent"

                                Text {
                                    anchors.centerIn: parent
                                    text: parent.dayNum
                                    color: parent.isToday ? root.notes.paletteBg
                                           : (parent.isCurrent ? root.notes.paletteFg
                                                                : root.withA(root.notes.paletteFg, 0.3))
                                    font.family: root.faceSerif
                                    font.pixelSize: 11
                                    font.bold: parent.isToday
                                }
                            }
                        }
                    }
                }
            }

            MouseArea {
                anchors.fill: parent
                anchors.margins: -4
                // Wheel pages months — up (positive angleDelta.y) steps back,
                // down steps forward.
                onWheel: {
                    root.monthOffset += (wheel.angleDelta.y > 0) ? -1 : 1
                    wheel.accepted = true
                }
            }
        }

        // ── Tally line: day N of M, kaomoji reads the browse state ──────────
        Item {
            width: parent.width
            height: 16
            Text {
                anchors.left: parent.left
                anchors.verticalCenter: parent.verticalCenter
                text: "day " + root.today + " of " + root.daysInMonth
                color: root.notes.paletteAccent
                font.family: root.faceMono
                font.pixelSize: 11
                font.bold: true
            }
            Text {
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                width: 96
                horizontalAlignment: Text.AlignRight
                text: root.kaomojiFor()
                font.family: root.faceMono
                font.pixelSize: 12
                color: root.withA(root.clay, 0.9)
            }
        }

        // ── TUI bottom frame: └─────────── 𝄂 ┘ — closes on the barline ────
        Item {
            width: parent.width
            height: 18
            Text {
                id: ffL
                anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter
                text: "└"
                font.family: root.faceMono; font.pixelSize: 11
                color: root.clay
            }
            Text {
                id: ffCorner
                anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                text: "┘"
                font.family: root.faceMono; font.pixelSize: 11
                color: root.clay
            }
            Text {
                id: ffBar
                anchors.right: ffCorner.left; anchors.rightMargin: 4
                anchors.verticalCenter: parent.verticalCenter
                text: "𝄂"
                font.family: root.faceMusic; font.pixelSize: 16
                color: root.notes.paletteAccent
            }
            Rectangle {
                anchors.left: ffL.right; anchors.right: ffBar.left
                anchors.leftMargin: 2; anchors.rightMargin: 6
                anchors.verticalCenter: parent.verticalCenter
                height: 1; color: root.withA(root.clay, 0.55)
            }
        }
    }
}
