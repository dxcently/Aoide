// calendar.qml — cadenza's `calendar` slot: a `cal`-style month grid in one
// termui pane, dropped from the bar's clock cell (intent §3.8).
//
//     ┌─ CAL ────── Sat 26 Sep ─┐
//     │ <    September 2026   > │    < > page a month; the wheel pages too
//     │ wk Mo Tu We Th Fr Sa Su │    labels dim
//     │ 36     1  2  3  4  5  6 │    ISO week numbers dim, days ink
//     │ 39 21 22 23 24 25 26 27 │    today: accent bold on the select band
//     └─────────────────────────┘
//
// ── SLOT ──────────────────────────────────────────────────────────────────
// WidgetSlot-hosted (root Item, no extras): cadenza's bar.qml loads it in
// its clock pane and sizes the popup from this item's implicit size; an
// implicitHeight of 0 (the Pane helper failed to load) hands the clock back
// to the bar's date pane. The size is fixed — six week rows always, as `cal`
// prints them — so paging never resizes the popup under the pointer.
//
// ── LOADING ───────────────────────────────────────────────────────────────
// No qmldir (design/kit.md §1): only `import "Kit.js" as Kit`, and the Pane
// is instantiated by URL. The grid is plain Texts, one per cell, calling kit
// values inline — no helper per day.
//
// ── GRID ──────────────────────────────────────────────────────────────────
// Weeks start on Monday and carry ISO-8601 week numbers (`ncal -w` / `cal
// -wm`): an ISO week is only well-defined Monday-first. Days outside the
// month are blank, as `cal` leaves them. Rows a month does not need stay
// blank, week number included.
//
// ── MOTION ────────────────────────────────────────────────────────────────
// The pane reveals once when the popup opens (the kit's 160ms pen). Paging
// swaps the grid whole — no slide, nothing animates at rest (intent §2).
// `today` re-reads itself once at local midnight; no per-second timer.
//
// ── NAVIGATION ────────────────────────────────────────────────────────────
// `<` / `>` page one month; the wheel pages one month per notch (up = back,
// as a scroll back in time); clicking the month label, or a middle click
// anywhere, returns to today. `monthOffset` is the only state: the bar
// recreates this item each time the pane opens, so it always opens on today.
import QtQuick
import "Kit.js" as Kit

Item {
    id: root

    required property var livery
    property var bridge: null          // handed by WidgetSlot; unused — a calendar needs no bridge

    // months from today's month; 0 = the current month
    property int monthOffset: 0

    // ── the kit ──────────────────────────────────────────────────────────
    FontMetrics { id: fm; font: Kit.bodyFont(13) }
    readonly property var kit: Kit.make(root.livery, fm)

    component Use: Loader {
        required property var kit
        required property string helper
        property var props: ({})
        Component.onCompleted: {
            var p = { kit: Qt.binding(() => kit) }
            for (var k in props) p[k] = props[k]
            setSource(kit.helper(helper), p)
        }
    }

    // ── today (re-read at local midnight) ────────────────────────────────
    property var today: new Date()
    function msToMidnight() {
        var n = new Date()
        var m = new Date(n.getFullYear(), n.getMonth(), n.getDate() + 1, 0, 0, 1)
        return Math.max(1000, m.getTime() - n.getTime())
    }
    Timer {
        id: midnight
        running: true
        interval: root.msToMidnight()
        onTriggered: {
            root.today = new Date()
            interval = root.msToMidnight()
            restart()
        }
    }

    // ── the shown month ──────────────────────────────────────────────────
    readonly property var shown: new Date(today.getFullYear(), today.getMonth() + monthOffset, 1)
    readonly property int shownYear: shown.getFullYear()
    readonly property int shownMonth: shown.getMonth()
    readonly property bool isHome: monthOffset === 0
    readonly property string monthLabel: Qt.locale("C").standaloneMonthName(shownMonth) + " " + shownYear

    // Monday-first column of a Date (0 = Mo … 6 = Su)
    function col(d) { return (d.getDay() + 6) % 7 }

    // ISO-8601 week number of y-m-d
    function isoWeek(y, m, d) {
        var t = new Date(Date.UTC(y, m, d))
        var dow = (t.getUTCDay() + 6) % 7
        t.setUTCDate(t.getUTCDate() - dow + 3)              // this week's Thursday
        var jan4 = new Date(Date.UTC(t.getUTCFullYear(), 0, 4))
        var jan4Mon = new Date(jan4.getTime() - ((jan4.getUTCDay() + 6) % 7) * 864e5)
        return 1 + Math.round((t.getTime() - jan4Mon.getTime()) / (7 * 864e5))
    }

    // six rows × seven days: { week: "36" | "", days: [0 | 1..31] }
    readonly property var weeks: {
        var lead = root.col(root.shown)
        var len = new Date(root.shownYear, root.shownMonth + 1, 0).getDate()
        var out = []
        for (var r = 0; r < 6; r++) {
            var days = [], first = 0
            for (var c = 0; c < 7; c++) {
                var d = r * 7 + c - lead + 1
                var v = (d >= 1 && d <= len) ? d : 0
                if (v && !first) first = v
                days.push(v)
            }
            out.push({ week: first ? String(root.isoWeek(root.shownYear, root.shownMonth, first)) : "",
                       days: days })
        }
        return out
    }
    readonly property int todayDay:
        (today.getFullYear() === shownYear && today.getMonth() === shownMonth) ? today.getDate() : 0

    function page(n) { root.monthOffset += n }
    function home() { root.monthOffset = 0 }

    // ── the grid's geometry, in cells ────────────────────────────────────
    // wk(2) · gap · 7 × dd with single-cell gaps = 23 inner cells
    readonly property int innerCols: 23
    readonly property int innerRows: 8            // nav · header · 6 weeks
    function dayX(c) { return root.kit.cells(3 + 3 * c) }

    implicitWidth: paneLoader.item ? paneLoader.implicitWidth : 0
    // Half a line of air above the pane: the title and stat are cut into the
    // top rule, whose line starts at the pane's y = 0, so without the inset
    // their glyph tops sit on the item's (and the popup's) top edge.
    readonly property int topInset: Math.round(kit.cellH / 2)
    implicitHeight: paneLoader.item ? topInset + paneLoader.implicitHeight : 0

    // the wheel, under everything; one page per 120 of delta (trackpads accumulate)
    property int _wheelAcc: 0
    MouseArea {
        anchors.fill: parent
        acceptedButtons: Qt.MiddleButton
        onClicked: root.home()
        onWheel: function (w) {
            root._wheelAcc += w.angleDelta.y
            while (root._wheelAcc >= 120) { root._wheelAcc -= 120; root.page(-1) }
            while (root._wheelAcc <= -120) { root._wheelAcc += 120; root.page(1) }
        }
    }

    Use {
        id: paneLoader
        y: root.topInset
        kit: root.kit
        helper: "Pane"
        props: ({
            title: "cal",
            stat: Qt.binding(() => Qt.formatDate(root.today, "ddd dd MMM")),
            statColor: Qt.binding(() => root.kit.mid),
            focused: true,
            animateOnCreate: true,
            cols: root.innerCols,
            rows: root.innerRows,
            content: calBody
        })
    }

    Component {
        id: calBody
        Item {
            width: root.kit.cells(root.innerCols)
            height: root.kit.lines(root.innerRows)

            // ── nav: <  Month Year  > ─────────────────────────────────────
            component Key: Text {
                id: key
                required property var kit
                signal activated()
                width: kit.cells(1)
                height: kit.cellH
                color: keyMa.containsMouse ? kit.title : kit.dim
                font: kit.font
                textFormat: Text.PlainText
                horizontalAlignment: Text.AlignHCenter
                style: Text.Outline
                styleColor: kit.withA(color, 0.18)
                MouseArea {
                    id: keyMa
                    // a whole-cell target plus a cell of slack each side
                    x: -key.kit.cellW; y: 0
                    width: key.kit.cells(3); height: key.height
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: key.activated()
                }
            }
            Key { kit: root.kit; x: 0; text: "<"; onActivated: root.page(-1) }
            Key { kit: root.kit; x: root.kit.cells(root.innerCols - 1); text: ">"; onActivated: root.page(1) }
            Text {
                id: label
                x: Math.round((root.kit.cells(root.innerCols) - implicitWidth) / 2)
                text: root.monthLabel
                color: (labelMa.containsMouse && !root.isHome) ? root.kit.title : root.kit.bright
                font: root.kit.font
                textFormat: Text.PlainText
                style: Text.Outline
                styleColor: root.kit.withA(color, 0.18)
                MouseArea {
                    id: labelMa
                    anchors.fill: parent
                    hoverEnabled: true
                    enabled: !root.isHome
                    cursorShape: root.isHome ? Qt.ArrowCursor : Qt.PointingHandCursor
                    onClicked: root.home()
                }
            }

            // ── header: wk Mo Tu … ────────────────────────────────────────
            Text {
                y: root.kit.cellH
                text: "wk Mo Tu We Th Fr Sa Su"
                color: root.kit.dim
                font: root.kit.font
                textFormat: Text.PlainText
            }

            // ── weeks ─────────────────────────────────────────────────────
            Repeater {
                model: 6
                Item {
                    id: row
                    required property int index
                    readonly property var w: root.weeks[index]
                    y: root.kit.lines(2 + index)
                    width: root.kit.cells(root.innerCols)
                    height: root.kit.cellH
                    Text {
                        width: root.kit.cells(2)
                        horizontalAlignment: Text.AlignRight
                        text: row.w.week
                        color: root.kit.dim
                        font: root.kit.font
                        textFormat: Text.PlainText
                    }
                    // today's cell: `cal`'s reverse video, drawn as the select band
                    // (half a cell of air each side) under the accent numeral
                    Rectangle {
                        readonly property int c: row.w.days.indexOf(root.todayDay)
                        visible: root.todayDay > 0 && c >= 0
                        x: root.dayX(Math.max(0, c)) - Math.round(root.kit.cellW / 2)
                        width: root.kit.cells(3)
                        height: root.kit.cellH
                        color: root.kit.select
                    }
                    Repeater {
                        model: 7
                        Text {
                            required property int index
                            readonly property int day: row.w.days[index]
                            readonly property bool isToday: day > 0 && day === root.todayDay
                            x: root.dayX(index)
                            width: root.kit.cells(2)
                            horizontalAlignment: Text.AlignRight
                            text: day > 0 ? String(day) : ""
                            color: isToday ? root.kit.title : root.kit.ink
                            font: isToday ? root.kit.titleFont : root.kit.font
                            textFormat: Text.PlainText
                            style: Text.Outline
                            styleColor: root.kit.withA(color, 0.18)
                        }
                    }
                }
            }
        }
    }
}
