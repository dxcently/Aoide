// calendar.qml — default/Pantheon "calendar" flavor widget.
//
// khoa, 2026-07-31: full rewrite of the proof-stub month grid into the
// "fuller calendar" pass, rendered in the default song's OWN hollow-wireframe
// idiom (song/songbook/default/design/pantheon.md §1 "the depth recipe") —
// this is NOT a reskin of sonata's Composite temple next door; it is a
// fundamentally different visual language. No Greek vocabulary (no order,
// no signature hue beyond the house roles, no crown glyph, no frieze
// ornament, no carved serif, no kaomoji) belongs to this song's grammar.
//
// The depth read: TWO offset outline ghost copies sit BEHIND the grid's
// hollow double-rule box — pantheon.md §1's verbatim constants (depthOff1
// 3px/depthOpacity1 0.35 near copy, depthOff2 6px/depthOpacity2 0.18 far
// copy, both holoBlue, border-only/no-fill) — the same "stacked offset
// volume" read every Pantheon panel wears. GadgetFrame's OWN depth stack is
// retired chrome-wide (song/songbook/sonata/design/greek-grammar.md §4 —
// GadgetFrame hosts the bar's popouts and stays flat), so this widget draws
// its own copies locally rather than relying on frame-level plumbing that
// no longer exists; `depthExtent` (6px) of extra right/bottom headroom is
// reserved on the root Item itself so the far ghost isn't clipped inside the
// BarPopout window.
//
// ── The "fuller calendar" mechanic (shared with sonata's sibling) ────────
// A FIXED 42-cell (6×7) grid, wheel-paged by month (`monthOffset`, no
// ListView/PathView) — always 6 rows, so the popup height never changes
// across months. Leading/trailing cells outside the viewed month show the
// adjacent month's days dimmed. `implicitHeight`/`implicitWidth` are FIXED
// constants, not `childrenRect`, for the same popup-stability reason as the
// sonata sibling.
//
// Fixed injected-prop contract: `notes` + `bridge` (bridge unused — a
// calendar reads nothing from shellbridge — but declared per the contract).

import QtQuick

Item {
    id: root

    required property var notes
    required property var bridge

    readonly property string faceMono: "JetBrainsMono Nerd Font"

    function withA(cstr, a) {
        var c = Qt.color(cstr)
        return Qt.rgba(c.r, c.g, c.b, a)
    }

    // ── pantheon.md §1 depth-recipe constants, VERBATIM ─────────────────────
    readonly property int depthOff1: 3
    readonly property int depthOff2: 6
    readonly property real depthOpacity1: 0.35
    readonly property real depthOpacity2: 0.18
    readonly property int depthExtent: 6   // headroom the far ghost needs

    // ── Fixed size — a constant, not childrenRect (see header) ──────────────
    implicitWidth: 240
    implicitHeight: 260

    property date now: new Date()
    Timer {
        interval: 60000
        running: true
        repeat: true
        onTriggered: root.now = new Date()
    }

    // ── Month paging state — identical mechanic to the sonata sibling ──────
    property int monthOffset: 0
    readonly property date viewedDate:
        new Date(root.now.getFullYear(), root.now.getMonth() + root.monthOffset, 1)
    readonly property int viewYear: viewedDate.getFullYear()
    readonly property int viewMonth: viewedDate.getMonth()
    readonly property int today: now.getDate()

    readonly property var monthNames: ["January", "February", "March", "April",
        "May", "June", "July", "August", "September", "October", "November", "December"]
    readonly property var dayLetters: ["S", "M", "T", "W", "T", "F", "S"]

    readonly property int daysInMonth: new Date(viewYear, viewMonth + 1, 0).getDate()
    readonly property int firstWeekday: new Date(viewYear, viewMonth, 1).getDay()
    readonly property int daysInPrevMonth: new Date(viewYear, viewMonth, 0).getDate()

    Column {
        id: mainColumn
        width: parent.width
        spacing: 8

        // ── Header: ‹ month year › — click resets to today ──────────────────
        Item {
            width: parent.width
            height: 20

            Text {
                id: prevArrow
                anchors.left: parent.left
                anchors.verticalCenter: parent.verticalCenter
                text: "‹"
                color: root.notes.wireCyan
                font.family: root.faceMono
                font.pixelSize: 15
                font.bold: true
                MouseArea {
                    anchors.fill: parent
                    anchors.margins: -4
                    cursorShape: Qt.PointingHandCursor
                    onClicked: root.monthOffset -= 1
                }
            }
            Text {
                id: titleText
                anchors.centerIn: parent
                text: root.monthNames[root.viewMonth] + " " + root.viewYear
                color: root.notes.paletteAccent
                font.family: root.faceMono
                font.pixelSize: 14
                font.bold: true
            }
            MouseArea {
                anchors.left: titleText.left; anchors.right: titleText.right
                anchors.top: titleText.top; anchors.bottom: titleText.bottom
                anchors.margins: -4
                cursorShape: Qt.PointingHandCursor
                onClicked: root.monthOffset = 0
            }
            Text {
                id: nextArrow
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                text: "›"
                color: root.notes.wireCyan
                font.family: root.faceMono
                font.pixelSize: 15
                font.bold: true
                MouseArea {
                    anchors.fill: parent
                    anchors.margins: -4
                    cursorShape: Qt.PointingHandCursor
                    onClicked: root.monthOffset += 1
                }
            }
        }

        // ── Weekday header — dims to 0.55 ────────────────────────────────
        Row {
            anchors.horizontalCenter: parent.horizontalCenter
            height: 16
            spacing: 2
            Repeater {
                model: root.dayLetters
                delegate: Text {
                    required property var modelData
                    width: 26
                    horizontalAlignment: Text.AlignHCenter
                    text: modelData
                    color: root.withA(root.notes.paletteFg, 0.55)
                    font.family: root.faceMono
                    font.pixelSize: 11
                    font.bold: true
                }
            }
        }

        // ── Depth stack + hollow grid box ────────────────────────────────
        // boxArea reserves depthExtent of extra room on its own right/bottom
        // so the far ghost copy (offset +depthOff2) isn't clipped.
        Item {
            id: boxArea
            anchors.horizontalCenter: parent.horizontalCenter
            width: box.width + root.depthExtent
            height: box.height + root.depthExtent

            // Ghost copies — declared BEFORE the main box so they render
            // behind it (only the sliver past the box's bottom-right edge
            // shows as a clean outline; the rest reads as a faint double-
            // rule ghost). Border-only, no fill, no MouseArea — never
            // intercepts input.
            Rectangle {
                id: farGhost
                x: root.depthOff2; y: root.depthOff2
                width: box.width; height: box.height
                radius: 0; color: "transparent"
                border.color: root.notes.holoBlue
                border.width: 1
                opacity: root.depthOpacity2
            }
            Rectangle {
                id: nearGhost
                x: root.depthOff1; y: root.depthOff1
                width: box.width; height: box.height
                radius: 0; color: "transparent"
                border.color: root.notes.holoBlue
                border.width: 1
                opacity: root.depthOpacity1
            }

            // ── The hollow double-rule box — outer + inset border, no fill ──
            Rectangle {
                id: box
                x: 0; y: 0
                width: grid.width + 16
                height: grid.height + 16
                radius: 0
                color: "transparent"
                border.color: root.notes.wireCyan
                border.width: 1

                Rectangle {
                    anchors.fill: parent
                    anchors.margins: 3
                    radius: 0
                    color: "transparent"
                    border.color: root.notes.wireCyan
                    border.width: 1
                    opacity: 0.6
                }

                // ── The fixed 42-cell (6×7) grid — wheel over it pages months
                Column {
                    id: grid
                    anchors.centerIn: parent
                    spacing: 3

                    Repeater {
                        model: 6
                        delegate: Row {
                            required property int index
                            readonly property int weekIdx: index
                            spacing: 3

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
                                    // The ONE neon — today, and only when not
                                    // browsing another month.
                                    readonly property bool isToday:
                                        root.monthOffset === 0 && isCurrent && dayNum === root.today

                                    width: 26
                                    height: 18
                                    radius: 0
                                    color: isToday ? root.notes.paletteHot : "transparent"

                                    Text {
                                        anchors.centerIn: parent
                                        text: parent.dayNum
                                        color: parent.isToday ? root.notes.paletteBg
                                               : (parent.isCurrent ? root.notes.paletteFg
                                                                    : root.withA(root.notes.paletteFg, 0.3))
                                        font.family: root.faceMono
                                        font.pixelSize: 11
                                        font.bold: parent.isToday
                                    }
                                }
                            }
                        }
                    }
                }
            }

            MouseArea {
                anchors.fill: box
                anchors.margins: -4
                // Wheel pages months — up (positive angleDelta.y) steps back,
                // down steps forward. Matches the sonata sibling's direction.
                onWheel: {
                    root.monthOffset += (wheel.angleDelta.y > 0) ? -1 : 1
                    wheel.accepted = true
                }
            }
        }
    }
}
