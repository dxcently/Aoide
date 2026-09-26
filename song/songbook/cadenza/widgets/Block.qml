// Block.qml — the borderless block (termui's magenta one): a single message
// that is not a pane — one herald toast, one board item.
//
//     herald ▸ firefox                    14:02
//     "Download finished: report.pdf"
//
// A HELPER (uppercase — never a slot), instantiated by URL
// (`kit.helper("Block")`, design/kit.md §1). API: design/kit.md §Block.
// No rules at all: a `kit.raised` (base01) field at paneAlpha, one cell of
// inset, an optional `label` line (tinted `tone`, default dim) with an
// optional dim right-hand `stamp`, then the `content` Component instantiated
// full-width below it. `selected` lifts the field to `kit.select`.
// Implicit height follows the content; width is `cols` + 2 cells unless sized.
import QtQuick

Item {
    id: block

    required property var kit
    property string label: ""
    property string stamp: ""
    property color tone: kit.dim
    property bool selected: false
    property int cols: 32
    property Component content: null
    readonly property Item contentItem: body.item

    implicitWidth: kit.cells(cols + 2)
    implicitHeight: inner.implicitHeight + kit.cellH     // half a line above + below

    Rectangle {
        anchors.fill: parent
        color: block.kit.withA(block.selected ? block.kit.select : block.kit.raised, block.kit.paneAlpha)
    }
    Column {
        id: inner
        x: block.kit.cellW
        y: Math.round(block.kit.cellH / 2)
        width: block.width - 2 * block.kit.cellW
        Item {
            visible: block.label.length > 0 || block.stamp.length > 0
            width: parent.width
            height: block.kit.cellH
            Text {
                anchors.left: parent.left
                anchors.right: stampText.left
                elide: Text.ElideRight
                text: block.label
                color: block.tone; font: block.kit.font; textFormat: Text.PlainText
            }
            Text {
                id: stampText
                anchors.right: parent.right
                text: block.stamp
                color: block.kit.dim; font: block.kit.font; textFormat: Text.PlainText
            }
        }
        Loader {
            id: body
            width: parent.width
            sourceComponent: block.content
        }
    }
}
