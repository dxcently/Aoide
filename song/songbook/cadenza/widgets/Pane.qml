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
// ── INNER GLOW ────────────────────────────────────────────────────────────
// `kit.title` phosphor blooms inward from all four edges and fades softly,
// visible ~28px in: the phosphor lit just inside the tube's frame (intent §2
// "Inner glow"). ONE Canvas below the top rule, between the fill and the
// rules, behind the content: a gaussian baked by shadowBlur, painted when
// the size or `kit.title` changes and never per frame. It runs under the
// title and the stat too (they sit on no box). Always `title`, whatever the
// rule's colour. Focus eases the canvas's opacity (rest = `_glowRest` of
// focused, 150ms); reveal/close fade it with the fill; neither repaints.
//
// The paint runs on the canvas's own render thread (`Canvas.Threaded`), never
// the GUI thread. Context2D's shadowBlur costs seconds per paint at a pane's
// size (about 4 s at 330x200, 12 s at the dock's 562x1043 frame), and on the
// GUI thread that froze the whole shell: the bar clock, the toasts, every
// shortcut. The work is the same on the worker, so a resize's glow lands
// late, and a pane whose height follows its content repaints on every change.
// `innerGlow: false` only hides the canvas; it still paints.
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
    property bool innerGlow: true        // title phosphor bleeding inward (INNER GLOW)
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

    // ── inner glow: one Canvas, a gaussian painted off the GUI thread ───────
    // A solid `title` frame is filled just OUTSIDE the pane rect with a
    // shadowBlur; only its blurred shadow falls inside the canvas, so the
    // light is a true gaussian from every edge, the corners round and even.
    // It repaints on size/colour change only; focus and reveal move its
    // `opacity`, which never repaints. `Canvas.Threaded` keeps the seconds a
    // shadowBlur costs off the GUI thread (header, INNER GLOW).
    readonly property real _glowBlur: 34          // Context2D shadowBlur
    readonly property real _glowPeak: 0.8         // shadowColor alpha (focused)
    readonly property real _glowRest: 0.5        // rest strength, of focused
    property real _glowLevel: focused ? 1 : _glowRest
    Behavior on _glowLevel { NumberAnimation { duration: 150 } }
    Canvas {
        id: glowCanvas
        x: 0; y: pane._ruleY
        width: pane._w; height: pane._side
        renderStrategy: Canvas.Threaded
        visible: pane.innerGlow && width > 0 && height > 0   // reveal moves opacity only
        opacity: pane.fillA / pane.kit.paneAlpha * pane._glowLevel
        readonly property color tint: pane.kit.title
        onTintChanged: requestPaint()
        onWidthChanged: requestPaint()
        onHeightChanged: requestPaint()
        onPaint: {
            var ctx = getContext("2d")
            ctx.reset()
            ctx.clearRect(0, 0, width, height)
            var t = 4 * pane._glowBlur          // frame thicker than the blur reach
            ctx.fillRule = Qt.OddEvenFill
            ctx.fillStyle = tint
            ctx.shadowColor = pane.kit.withA(tint, pane._glowPeak)
            ctx.shadowBlur = pane._glowBlur
            ctx.beginPath()
            ctx.rect(-t, -t, width + 2 * t, height + 2 * t)
            ctx.rect(0, 0, width, height)
            ctx.fill()
        }
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
