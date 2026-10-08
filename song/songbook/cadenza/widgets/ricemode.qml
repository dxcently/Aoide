// ricemode.qml — cadenza's "ricemode" slot: the RICE cell and its menu.
//
// bar.qml embeds it as `WidgetSlot { slot: "ricemode" }` between the TRAY cell
// and the clock. The look is cadenza's — an icon and a word on the character
// cell, the kit's colours, the menu a kit Pane — and the contract is the one
// every `ricemode` body keeps (sonata's is the floor), through the same bridge
// verbs. Left, right and middle click all do one thing: open the menu, or close
// it. The cell never switches; nothing changes until a row is chosen.
// bridge.riceMenu(cb) fills the Pane, top to bottom:
//   songs                  the runtime songbook; a row sends
//                          bridge.riceMode("stage", name). A song that does not
//                          load is a dim `<name> · unreadable` row that does nothing
//                          (the reason does not fit the 22-cell row)
//   [declarative] <song>   sends bridge.riceMode("declarative")
//   drafts · <song>        the staged song's drafts, a row sends
//                          bridge.riceDraft("enter", name); the two fixed rows
//                          send ("new") and ("save"); `save` is offered in
//                          staging only (in draft mode writes already land in
//                          the draft, in declarative nothing is unsaved)
// The lamp column marks what is live: a full lamp for the song staged now (in
// the staging colour), for the declarative row while it is locked, and for the
// current draft; a hollow lamp, dim, on the last-staged song while locked. The
// marked rows only close the menu. In draft mode the drafted song's row is live:
// it leaves the draft for plain staging.
// A stage, a declarative, an enter and a new each change what is live, so the
// word reads a dim `…` and every click is swallowed until livery.riceMode, the
// draft name or the song changes, or 10s pass: the double click that sent two
// switches. A `save` changes nothing and holds nothing.
//
// The menu is marked by a chevron after the word (kit.glyph.ricePick, the icon
// table's one entry for it), so it is on screen at rest. A bridge without
// `riceMenu` has no menu: the cell is inert, no chevron, no click.
//
// The menu is a PopupWindow made on open and destroyed on close, so no hover
// state carries into the next open. It closes on a row action, on any click on
// the cell, and 600ms after the pointer is over neither the cell nor the
// popup. No text input, so no keyboard and no focus grab: the CLI mints a new
// draft's name. Its Pane has no inner glow: the canvas queues a multi-second
// paint on the render thread every Threaded canvas shares (CoverPcb's too), and
// a menu that is rebuilt on every open would queue one each time. Every
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
    readonly property string song: root.livery.songName
    readonly property bool canMenu: !!root.bridge && typeof root.bridge.riceMenu === "function"

    property bool pending: false       // a mode change was sent and has not landed
    property bool open: false          // the menu
    property var answer: null          // the last riceMenu reply, null while asking
    property int asked: 0              // which ask a reply belongs to
    property Item hotRow: null         // the menu row under the pointer
    readonly property bool ready: !!root.answer && root.answer.ok === true
    readonly property var songs: root.ready && Array.isArray(root.answer.songs) ? root.answer.songs : []
    readonly property var drafts: root.ready && Array.isArray(root.answer.drafts) ? root.answer.drafts : []
    readonly property string note: !root.answer ? "…"
        : (!root.answer.ok ? "" + (root.answer.message || "no answer") : "")
    readonly property int pickLines: 1 + (root.ready ? root.songs.length : 1) + 2
                                     + (root.ready && root.drafts.length === 0 ? 1 : 0) + root.drafts.length
                                     + 1 + (root.mode === "staging" ? 1 : 0)

    onModeChanged: root.pending = false
    onDraftChanged: root.pending = false
    onSongChanged: root.pending = false
    Timer { interval: 10000; running: root.pending; onTriggered: root.pending = false }

    function gesture() {
        if (root.pending) return
        if (root.open) {
            root.open = false
            return
        }
        root.answer = null
        var ask = ++root.asked
        root.open = true
        root.bridge.riceMenu(function (reply) { if (ask === root.asked) root.answer = reply })
    }

    // `save` never changes what is live; the rest do.
    function act(kind, name) {
        root.open = false
        if (kind !== "save") root.pending = true
        if (kind === "stage") root.bridge.riceMode("stage", name)
        else if (kind === "declarative") root.bridge.riceMode("declarative")
        else root.bridge.riceDraft(kind, name)
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

    implicitWidth: root.kit.cells(2 + root.word.length + (root.canMenu ? 2 : 0))
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
        visible: root.canMenu
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
        enabled: root.canMenu
        acceptedButtons: Qt.LeftButton | Qt.RightButton | Qt.MiddleButton
        cursorShape: root.canMenu ? Qt.PointingHandCursor : Qt.ArrowCursor
        onClicked: root.gesture()
    }

    // ── The menu ────────────────────────────────────────────────────────────
    // One row of the pane: a lamp column, a name, a hover band, tap.
    component PickRow: Item {
        id: row
        property string lamp: ""
        property color lampColor: root.kit.dim
        property string name: ""
        property color nameColor: root.kit.ink
        property bool live: true          // false for a header or a message
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
                live: false
                name: "songs"
                nameColor: root.kit.dim
            }
            PickRow {
                visible: !root.ready
                live: false
                name: root.note
                nameColor: root.kit.dim
            }
            Repeater {
                model: root.songs
                PickRow {
                    required property var modelData
                    readonly property bool here: root.ready && root.mode === "staging" && modelData.name === root.answer.song
                    readonly property bool last: root.ready && root.mode === "declarative" && modelData.name === root.answer.stagingSong
                    live: !!modelData.ok
                    lamp: here ? root.kit.lampGlyph("working") : (last ? root.kit.lampGlyph("idle") : "")
                    lampColor: here ? root.modeColor("staging") : root.kit.dim
                    name: modelData.ok ? "" + modelData.name : modelData.name + " · unreadable"
                    nameColor: modelData.ok ? root.kit.ink : root.kit.dim
                    onActivated: {
                        if (here) root.open = false
                        else root.act("stage", "" + modelData.name)
                    }
                }
            }
            PickRow {
                lamp: root.mode === "declarative" ? root.kit.lampGlyph("working") : ""
                lampColor: root.modeColor("declarative")
                name: root.ready && root.answer.declared ? "[declarative] " + root.answer.declared : "[declarative]"
                nameColor: root.kit.title
                onActivated: {
                    if (root.mode === "declarative") root.open = false
                    else root.act("declarative")
                }
            }
            PickRow {
                live: false
                name: root.ready && root.answer.draftsSong ? "drafts · " + root.answer.draftsSong : "drafts"
                nameColor: root.kit.dim
            }
            PickRow {
                visible: root.ready && root.drafts.length === 0
                live: false
                name: "no saved drafts"
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
