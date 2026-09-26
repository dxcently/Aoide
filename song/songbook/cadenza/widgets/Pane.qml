// Pane.qml — the termui pane: cadenza's one box.
//
//     ┌─ AGENTS ─────────────── 3/5 ┐   title cut into the top rule (left),
//     │ ● rook      working  12s    │   an optional stat cut in at the right
//     └─────────────────────────────┘
//
// A HELPER (uppercase — never a slot), instantiated BY URL, never by type
// name (design/kit.md §1):
//
//     Loader {
//         Component.onCompleted: setSource(root.kit.helper("Pane"), {
//             kit: Qt.binding(() => root.kit), title: "agents",
//             stat: Qt.binding(() => live + "/" + total), cols: 38, rows: 6,
//             content: agentsBody })
//     }
//     Component { id: agentsBody; Column { … } }   // ids of the widget resolve inside
//
// Full API: design/kit.md §Pane.
//
// ── RULES ARE HAIRLINES, NOT TEXT ─────────────────────────────────────────
// 1px Rectangles (intent §2): box-drawing text cannot close at a fractional
// scale. The top rule is cut into three runs — lead-in stub, the run between
// title and stat, the tail — so title and stat sit IN the rule with half a
// cell of air either side. Rest `kit.dim` (base03); `focused` → `kit.title`.
//
// ── REVEAL / CLOSE ────────────────────────────────────────────────────────
// `open: true` draws the rule clockwise from the top-left corner at constant
// pen speed (each edge's share of 160ms is its share of the perimeter), then
// the fill + content arrive (60ms). `open: false` runs the pen back and
// drops the fill together, 120ms. A pane created open skips the animation
// unless `animateOnCreate` — a list delegate must never animate.
//
// ── SIZE ──────────────────────────────────────────────────────────────────
// Whole cells: `cols`/`rows` are the INNER content size; the pane reports
// implicitWidth = cols+2 cells, implicitHeight = rows+1.5 lines (the top rule
// rides the middle of the title line, the bottom rule half a line under the
// last row). Sized from outside instead, read `innerCols`/`innerRows`.
// Content is instantiated from `content` (a Component) into the body: inset
// one cell left/right, one line from the top, clipped.
import QtQuick

Item {
    id: pane

    required property var kit
    property string title: ""
    property string stat: ""
    property color statColor: kit.number
    property bool focused: false
    property bool open: true
    property bool animateOnCreate: false
    property string glow: "outline"      // title glow: "off" | "outline" | "bloom"
    property Component content: null

    property int cols: 20
    property int rows: 3
    implicitWidth: kit.cells(cols + 2)
    implicitHeight: Math.round(kit.cellH * (rows + 1.5))

    readonly property int innerCols: Math.max(0, kit.fit(width) - 2)
    readonly property int innerRows: Math.max(0, Math.floor((height - 1.5 * kit.cellH) / kit.cellH))
    readonly property Item contentItem: bodyLoader.item

    // ── state (plain values: the animations own them) ─────────────────────
    property real pen: 0        // 0..1 of the perimeter drawn
    property real fillA: 0
    property color ruleColor: focused ? kit.title : kit.dim
    Behavior on ruleColor { ColorAnimation { duration: 150 } }

    Component.onCompleted: {
        if (!open) return
        if (animateOnCreate) revealAnim.start()
        else { pen = 1; fillA = kit.paneAlpha }
    }
    onOpenChanged: {
        if (open) { closeAnim.stop(); revealAnim.start() }
        else      { revealAnim.stop(); closeAnim.start() }
    }
    SequentialAnimation {
        id: revealAnim
        NumberAnimation { target: pane; property: "pen"; to: 1; duration: 160; easing.type: Easing.Linear }
        NumberAnimation { target: pane; property: "fillA"; to: pane.kit.paneAlpha; duration: 60; easing.type: Easing.OutCubic }
    }
    ParallelAnimation {
        id: closeAnim
        NumberAnimation { target: pane; property: "pen"; to: 0; duration: 120; easing.type: Easing.InQuad }
        NumberAnimation { target: pane; property: "fillA"; to: 0; duration: 120; easing.type: Easing.InQuad }
    }

    // ── perimeter geometry (px), clockwise from the top-left corner ───────
    readonly property real _w: Math.round(width)
    readonly property real _h: Math.round(height)
    readonly property real _ruleY: Math.round(kit.cellH / 2)
    readonly property real _side: _h - _ruleY
    readonly property real _drawn: pen * (2 * _w + 2 * _side)
    function _seg(start, len) { return Math.max(0, Math.min(len, _drawn - start)) }

    readonly property real _lead: kit.cellW
    readonly property real _pad: Math.round(kit.cellW / 2)
    readonly property real _titleX: _lead + _pad
    readonly property real _titleW: titleLoader.item ? titleLoader.item.implicitWidth : 0
    readonly property real _titleEnd: title.length ? _titleX + _titleW + _pad : _lead
    readonly property real _statX: stat.length ? _w - kit.cellW - _pad - statText.implicitWidth : _w
    readonly property real _statStart: stat.length ? _statX - _pad : _w
    readonly property real _tailX: stat.length ? _statX + statText.implicitWidth + _pad : _w

    // ── fill ───────────────────────────────────────────────────────────────
    Rectangle {
        x: 0; y: pane._ruleY; width: pane._w; height: pane._side
        color: pane.kit.withA(pane.kit.ground, pane.fillA)
    }

    // ── rules ──────────────────────────────────────────────────────────────
    Rectangle { x: 0; y: pane._ruleY; height: 1; color: pane.ruleColor
        width: pane._seg(0, Math.min(pane._lead, pane._w)) }
    Rectangle { x: pane._titleEnd; y: pane._ruleY; height: 1; color: pane.ruleColor
        width: pane._seg(pane._titleEnd, Math.max(0, pane._statStart - pane._titleEnd)) }
    Rectangle { x: pane._tailX; y: pane._ruleY; height: 1; color: pane.ruleColor
        width: pane.stat.length ? pane._seg(pane._tailX, Math.max(0, pane._w - pane._tailX)) : 0 }
    Rectangle { x: pane._w - 1; y: pane._ruleY; width: 1; color: pane.ruleColor
        height: pane._seg(pane._w, pane._side) }
    Rectangle { y: pane._h - 1; height: 1; color: pane.ruleColor
        width: pane._seg(pane._w + pane._side, pane._w); x: pane._w - width }
    Rectangle { x: 0; width: 1; color: pane.ruleColor
        height: pane._seg(2 * pane._w + pane._side, pane._side); y: pane._h - height }

    // ── title (GlowText, itself by URL) and stat, cut into the top rule ──
    Loader {
        id: titleLoader
        x: pane._titleX; y: 0
        active: pane.title.length > 0
        opacity: pane._drawn >= pane._lead ? 1 : 0
        Component.onCompleted: setSource(pane.kit.helper("GlowText"), {
            kit: Qt.binding(() => pane.kit),
            text: Qt.binding(() => pane.title.toUpperCase()),
            color: Qt.binding(() => pane.kit.title),
            bold: true,
            glow: Qt.binding(() => pane.glow) })
    }
    Text {
        id: statText
        x: pane._statX; y: 0
        visible: pane.stat.length > 0
        opacity: pane._drawn >= pane._statStart ? 1 : 0
        text: pane.stat
        color: pane.statColor
        font: pane.kit.font
        textFormat: Text.PlainText
    }

    // ── content ────────────────────────────────────────────────────────────
    Item {
        x: pane.kit.cellW
        y: pane.kit.cellH
        width: Math.max(0, pane._w - 2 * pane.kit.cellW)
        height: Math.max(0, pane._h - pane.kit.cellH - Math.round(pane.kit.cellH / 2))
        clip: true
        opacity: pane.fillA > 0 ? 1 : 0
        Loader {
            id: bodyLoader
            width: parent.width
            sourceComponent: pane.content
        }
    }
}
