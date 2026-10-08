// ricemode.qml — cadenza's "ricemode" slot: the RICE cell and its draft picker.
//
// bar.qml embeds it as `WidgetSlot { slot: "ricemode" }` between the TRAY cell
// and the clock. The look is cadenza's — an icon and a word on the character
// cell, the kit's colours, the picker a kit Pane — and the contract is the one
// every `ricemode` body keeps (sonata's is the floor), through the same bridge
// verbs:
//   left click             bridge.toggleRiceMode()   staging ⇄ declarative
//   right OR middle click  the draft picker: bridge.riceDrafts(cb) lists the
//                          staged song's drafts; a row sends
//                          bridge.riceDraft("enter", name) and the two fixed
//                          rows send ("new") and ("save"); `save` is offered in
//                          staging only (in draft mode writes already land in
//                          the draft, in declarative nothing is unsaved)
// A toggle, an enter and a new each change the mode, so the word reads a dim
// `…` and every click is swallowed until livery.riceMode or the draft name
// changes, or 10s pass: the double click that sent two toggles. A `save`
// changes no mode and holds nothing.
//
// The second gesture is marked by a chevron after the word (kit.glyph.ricePick,
// the icon table's one entry for it), so it is on screen at rest. A bridge
// without `riceDrafts` has no picker and no chevron.
//
// The picker is a PopupWindow made on open and destroyed on close, so no hover
// state carries into the next open. It closes on a row action, on any click on
// the cell, and 600ms after the pointer is over neither the cell nor the
// popup. No text input, so no keyboard and no focus grab: the CLI mints a new
// draft's name. Its Pane has no inner glow: the canvas queues a multi-second
// paint on the render thread every Threaded canvas shares (CoverPcb's too), and
// a picker that is rebuilt on every open would queue one each time. Every
// string that came over the bridge is painted PlainText.
//
// Footprint: as tall as the bar's line (barH, bar.qml's own), so the cell's
// bottom edge is the trunk and the picker hangs from it like every other pane.
import QtQuick
import Quickshell
import "Kit.js" as Kit

Item {
    id: root

    required property var livery
    required property var bridge

    readonly property int barH: 28
    readonly property int pickCols: 24

    FontMetrics { id: fm; font: Kit.bodyFont(13) }
    readonly property var kit: Kit.make(root.livery, fm)

    function modeWord(m) { return m === "staging" ? "stg" : (m === "draft" ? "drft" : "decl") }
    function modeColor(m) { return m === "staging" ? root.kit.title : (m === "draft" ? root.kit.path : root.kit.dim) }

    readonly property string mode: root.livery.riceMode || "declarative"
    readonly property string draft: (root.livery.modeRaw && root.livery.modeRaw.draft) || ""
    readonly property bool canPick: !!root.bridge && typeof root.bridge.riceDrafts === "function"

    property bool pending: false       // a mode change was sent and has not landed
    property bool open: false          // the picker
    property var answer: null          // the last riceDrafts reply, null while asking
    property int asked: 0              // which ask a reply belongs to
    property Item hotRow: null         // the picker row under the pointer
    readonly property var drafts: root.answer && root.answer.ok && Array.isArray(root.answer.drafts)
                                  ? root.answer.drafts : []
    readonly property string note: !root.answer ? "…"
        : (!root.answer.ok ? "" + (root.answer.message || "no answer")
           : (root.drafts.length === 0 ? "no saved drafts" : ""))
    readonly property int pickLines: (root.note !== "" ? 1 : 0) + root.drafts.length + 1
                                     + (root.mode === "staging" ? 1 : 0)

    onModeChanged: root.pending = false
    onDraftChanged: root.pending = false
    Timer { interval: 10000; running: root.pending; onTriggered: root.pending = false }

    function gesture(button) {
        var wasOpen = root.open
        root.open = false
        if (root.pending) return
        if (button === Qt.LeftButton) {
            root.pending = true
            root.bridge.toggleRiceMode()
        } else if (root.canPick && !wasOpen) {
            root.answer = null
            var ask = ++root.asked
            root.open = true
            root.bridge.riceDrafts(function (reply) { if (ask === root.asked) root.answer = reply })
        }
    }

    // `save` never changes the mode; `enter` and `new` do.
    function act(action, name) {
        root.open = false
        if (action !== "save") root.pending = true
        root.bridge.riceDraft(action, name)
    }

    // ── The cell ────────────────────────────────────────────────────────────
    // Same face as the bar's other cells: the glyph takes the word's colour,
    // and `title` while the picker is open or the pointer is on it. Pending
    // reads `…` padded to the word it replaces, so the clock never moves.
    readonly property string word: root.pending
        ? "…" + root.kit.rep(" ", root.modeWord(root.mode).length - 1)
        : root.modeWord(root.mode)
    readonly property color ink: root.pending ? root.kit.dim : root.modeColor(root.mode)
    readonly property bool lit: root.open || cellMa.containsMouse

    implicitWidth: root.kit.cells(2 + root.word.length + (root.canPick ? 2 : 0))
    implicitHeight: root.barH

    Text {
        text: root.kit.glyph.rice
        color: root.lit ? root.kit.title : root.ink
        font: root.kit.font
        textFormat: Text.PlainText
        style: Text.Outline
        styleColor: root.kit.withA(color, 0.18)
    }
    Text {
        x: root.kit.cells(2)
        text: root.word
        color: root.ink
        font: root.kit.font
        textFormat: Text.PlainText
        style: Text.Outline
        styleColor: root.kit.withA(color, 0.18)
    }
    Text {
        visible: root.canPick
        x: root.kit.cells(3 + root.word.length)
        y: 1
        text: root.kit.glyph.ricePick
        color: root.lit ? root.kit.title : root.kit.dim
        font: root.kit.font
        textFormat: Text.PlainText
    }
    MouseArea {
        id: cellMa
        anchors.fill: parent
        hoverEnabled: true
        acceptedButtons: Qt.LeftButton | Qt.RightButton | Qt.MiddleButton
        cursorShape: Qt.PointingHandCursor
        onClicked: function (mouse) { root.gesture(mouse.button) }
    }

    // ── The picker ──────────────────────────────────────────────────────────
    // One row of the pane: a lamp column, a name, a hover band, tap.
    component PickRow: Item {
        id: row
        property string lamp: ""
        property color lampColor: root.kit.dim
        property string name: ""
        property color nameColor: root.kit.ink
        property bool live: true          // false for a message
        readonly property bool hot: root.hotRow === row
        signal activated()

        width: root.kit.cells(root.pickCols)
        height: root.kit.cellH

        Rectangle { anchors.fill: parent; color: root.kit.select; visible: row.live && row.hot }
        Text {
            text: row.lamp
            color: row.lampColor
            font: root.kit.font
            textFormat: Text.PlainText
        }
        Text {
            x: root.kit.cells(2)
            // +2px: a run of n glyphs is a hair wider than round(n·cellW); never elide a fit
            width: root.kit.cells(root.pickCols - 2) + 2
            text: row.name.replace(/\s+/g, " ")
            color: row.live && row.hot ? root.kit.match : row.nameColor
            elide: Text.ElideRight
            font: root.kit.font
            textFormat: Text.PlainText
        }
        HoverHandler {
            enabled: row.live
            onHoveredChanged: {
                if (hovered) root.hotRow = row
                else if (root.hotRow === row) root.hotRow = null
            }
        }
        TapHandler {
            enabled: row.live
            onTapped: row.activated()
        }
    }

    Component {
        id: pickBody
        Column {
            PickRow {
                visible: root.note !== ""
                live: false
                name: root.note
                nameColor: root.kit.dim
            }
            Repeater {
                model: root.drafts
                PickRow {
                    required property var modelData
                    lamp: modelData.current ? root.kit.lampGlyph("working") : ""
                    lampColor: root.modeColor("draft")
                    name: "" + modelData.name
                    onActivated: {
                        if (modelData.current) root.open = false
                        else root.act("enter", "" + modelData.name)
                    }
                }
            }
            PickRow {
                name: "[+ new draft]"
                nameColor: root.kit.title
                onActivated: root.act("new")
            }
            PickRow {
                visible: root.mode === "staging"
                name: "[save as draft]"
                nameColor: root.kit.title
                onActivated: root.act("save")
            }
        }
    }

    LazyLoader {
        active: root.open

        PopupWindow {
            visible: true
            color: "transparent"
            anchor.item: root
            anchor.edges: Edges.Bottom | Edges.Right
            anchor.gravity: Edges.Bottom | Edges.Left
            anchor.adjustment: PopupAdjustment.FlipY | PopupAdjustment.Slide
            implicitWidth: Math.max(1, paneLoader.implicitWidth)
            implicitHeight: Math.max(1, paneLoader.implicitHeight)

            // The pointer has left both the cell and the popup: 600ms of grace
            // for the hand-off between two surfaces, then it closes.
            Timer {
                interval: 600
                running: !cellMa.containsMouse && !paneHover.hovered && root.hotRow === null
                onTriggered: root.open = false
            }

            Loader {
                id: paneLoader
                Component.onCompleted: setSource(root.kit.helper("Pane"), {
                    kit: Qt.binding(() => root.kit),
                    title: "rice",
                    stat: Qt.binding(() => root.answer && root.answer.ok ? "" + (root.answer.song || "no song") : ""),
                    focused: true,
                    animateOnCreate: true,
                    innerGlow: false,
                    cols: root.pickCols,
                    rows: Qt.binding(() => root.pickLines),
                    content: pickBody
                })
                HoverHandler { id: paneHover }
            }
        }
    }
}
