// ricemode.qml -- fugue's "ricemode" slot: the `mode` cell and its draft
// picker, hosted by bar.qml's WidgetSlot as the first cell of the right row
// (checks.bar-ricemode fails a bar that does not embed it).
//
// The look is fugue's: Cell instances on the lattice, a flat plate, a 2px rule
// for what is live, no ornament. The contract is the one every `ricemode`
// body keeps (sonata's is the floor), through the same bridge verbs:
//   left click             bridge.toggleRiceMode()   staging <-> declarative
//   right OR middle click  the draft picker: bridge.riceDrafts(cb) lists the
//                          staged song's drafts; a row sends
//                          bridge.riceDraft("enter", name) and the two fixed
//                          rows send ("new") and ("save"); `save` is offered in
//                          staging only (in draft mode writes already land in
//                          the draft, in declarative nothing is unsaved)
// A toggle, an enter and a new each change the mode, so the word reads `...`
// and every click is swallowed until livery.riceMode or the draft name changes,
// or 10s pass: the double click that sent two toggles. A `save` changes no mode
// and holds nothing.
//
// The second gesture is marked by a narrow cell after `mode`, holding a drawn
// chevron (two 1px rules, never a glyph: nothing on this lattice relies on a
// glyph nobody has seen render), so it is on screen at rest. The two cells are
// one control: a left click on either toggles. A bridge without `riceDrafts`
// has no picker and no chevron cell.
//
// The picker is a PopupWindow made on open and destroyed on close, so no hover
// state carries into the next open. It closes on a row action, on any click on
// the cells, and 600ms after the pointer is over neither the cells nor the
// popup. No text input, so no keyboard and no focus grab: the CLI mints a new
// draft's name. Every string that came over the bridge is painted PlainText,
// which is why its rows are not Cell instances (Cell's Text is AutoText).
//
// Cell is fugue's own uppercase helper, resolved by type name through the
// qmldir the build writes beside this file (bar.qml and herald.qml do the same).
import QtQuick
import Quickshell

Item {
    id: root

    required property var livery
    required property var bridge

    function modeWord(m) {
        if (m === "staging") return "stage"
        if (m === "draft") return "draft"
        return "decl"
    }

    readonly property string mode: root.livery.riceMode
    readonly property string draft: (root.livery.modeRaw && root.livery.modeRaw.draft) || ""
    readonly property bool canPick: !!root.bridge && typeof root.bridge.riceDrafts === "function"

    property bool pending: false       // a mode change was sent and has not landed
    property bool open: false          // the picker
    property var answer: null          // the last riceDrafts reply, null while asking
    property int asked: 0              // which ask a reply belongs to
    property Item hotRow: null         // the picker row under the pointer
    readonly property var drafts: root.answer && root.answer.ok && Array.isArray(root.answer.drafts)
                                  ? root.answer.drafts : []

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

    // -- The cells -----------------------------------------------------------
    // Pending reads `...` padded to the word it replaces, so nothing moves.
    implicitWidth: cells.implicitWidth
    implicitHeight: cells.implicitHeight
    opacity: root.pending ? 0.55 : 1.0

    Row {
        id: cells
        spacing: 0

        Cell {
            id: modeCell
            livery: root.livery
            paper: root.livery.barBg
            ink: root.livery.barFg
            label: "mode"
            value: root.pending
                   ? "..." + " ".repeat(root.modeWord(root.mode).length - 3)
                   : root.modeWord(root.mode)
            active: root.mode === "staging"
            interactive: true
            onActivated: root.gesture(Qt.LeftButton)
        }
        Cell {
            id: pickCell
            visible: root.canPick
            livery: root.livery
            paper: root.livery.barBg
            ink: root.livery.barFg
            interactive: true
            onActivated: root.gesture(Qt.LeftButton)

            // The mark: a 7x4 chevron of two 1px rules meeting at the bottom.
            Item {
                anchors.centerIn: parent
                width: 7
                height: 4
                Repeater {
                    model: [40.6, -40.6]
                    Rectangle {
                        required property real modelData
                        width: 5
                        height: 1
                        x: modelData > 0 ? -0.75 : 2.75
                        y: 1.5
                        rotation: modelData
                        antialiasing: true
                        color: pickCell.hot || root.open ? root.livery.paletteAccent : root.livery.barFg
                        opacity: pickCell.hot || root.open ? 1.0 : 0.6
                    }
                }
            }
        }
    }

    // Right and middle go to the picker. Left is not accepted here, so it
    // reaches the Cells beneath, which keep their own hover and rule.
    MouseArea {
        id: pickMa
        anchors.fill: parent
        acceptedButtons: Qt.RightButton | Qt.MiddleButton
        cursorShape: Qt.PointingHandCursor
        onClicked: function (mouse) { root.gesture(mouse.button) }
    }
    HoverHandler { id: cellsHover }

    // -- The picker ----------------------------------------------------------
    // One row of the plate: a label, a hover tone, a 2px rule when marked, tap.
    component PickRow: Item {
        id: row
        property string label: ""
        property real ink: 1.0
        property bool marked: false       // the draft the stage is routed into
        property bool live: true          // false for a message
        readonly property bool hot: root.hotRow === row
        signal activated()

        width: parent ? parent.width : 0
        height: 22

        Rectangle {
            visible: row.marked
            anchors.left: parent.left
            anchors.top: parent.top
            anchors.bottom: parent.bottom
            width: 2
            color: root.livery.paletteAccent
        }
        Text {
            x: 9
            width: parent.width - 18
            anchors.verticalCenter: parent.verticalCenter
            text: row.label
            textFormat: Text.PlainText
            elide: Text.ElideRight
            font.family: "JetBrainsMono Nerd Font"
            font.pixelSize: 11
            color: row.live && row.hot ? root.livery.paletteAccent : root.livery.barFg
            opacity: row.ink
            Behavior on color { ColorAnimation { duration: 120; easing.type: Easing.Linear } }
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

    LazyLoader {
        active: root.open

        PopupWindow {
            visible: true
            color: "transparent"
            anchor.item: root
            anchor.edges: Edges.Bottom | Edges.Right
            anchor.gravity: Edges.Bottom | Edges.Left
            anchor.adjustment: PopupAdjustment.FlipY | PopupAdjustment.Slide

            // A 4px gap under the cells.
            implicitWidth: plate.width
            implicitHeight: plate.height + 4

            // The pointer has left both the cells and the popup: 600ms of grace
            // for the hand-off between two surfaces, then it closes.
            Timer {
                interval: 600
                running: !cellsHover.hovered && !plateHover.hovered && root.hotRow === null
                onTriggered: root.open = false
            }

            Rectangle {
                id: plate
                y: 4
                width: 208
                height: rows.implicitHeight + 2
                radius: 0
                color: root.livery.barBg
                border.width: 1
                border.color: root.livery.windowBorderInactive

                HoverHandler { id: plateHover }

                Column {
                    id: rows
                    x: 1
                    y: 1
                    width: parent.width - 2

                    PickRow {
                        live: false
                        ink: 0.6
                        label: root.answer
                               ? (root.answer.ok ? (root.answer.song || "no song") : (root.answer.message || "no answer"))
                               : "..."
                    }
                    PickRow {
                        visible: root.answer !== null && root.answer.ok === true && root.drafts.length === 0
                        live: false
                        ink: 0.6
                        label: "no saved drafts"
                    }
                    Repeater {
                        model: root.drafts
                        PickRow {
                            required property var modelData
                            label: "" + modelData.name
                            marked: !!modelData.current
                            onActivated: {
                                if (modelData.current) root.open = false
                                else root.act("enter", "" + modelData.name)
                            }
                        }
                    }
                    PickRow {
                        label: "+ new draft"
                        onActivated: root.act("new")
                    }
                    PickRow {
                        visible: root.mode === "staging"
                        label: "save current as draft"
                        onActivated: root.act("save")
                    }
                }
            }
        }
    }
}
