// BrailleChart.qml — a line chart in the U+2800 block, 2×4 dots per cell.
//
//     $4.20 ⡀⢀⣀⠤⠒⠉⠁⠈⠑⠢⢄⡀
//      0.00 ⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉⠉
//
// A HELPER (uppercase — never a slot), instantiated by URL
// (`kit.helper("BrailleChart")`, design/kit.md §1). API: design/kit.md §BrailleChart.
// `cols` × `rows` cells. One Text per row, each line pinned to the cell grid
// (the face carries U+2800–28FF natively, so the dots stay on the grid).
// `lo`/`hi` fix the y range (null = auto from the data). With `axis: true`
// a dim y-axis gutter of `axisCells` shows hi on the top row, lo on the bottom.
import QtQuick

Row {
    id: chart

    required property var kit
    property var values: []
    property int cols: 24
    property int rows: 4
    property var lo: null
    property var hi: null
    property bool axis: true
    property int axisCells: 6
    property var format: function(v) { return Math.abs(v) >= 1000 ? (v / 1000).toFixed(1) + "k" : (Math.round(v * 100) / 100).toString() }
    property color color: kit.ink

    readonly property var _lines: kit.brailleLines(values, cols, rows, lo, hi)
    readonly property real _lo: {
        if (lo !== null && lo !== undefined) return lo
        var m = Infinity; for (var i = 0; i < (values || []).length; i++) m = Math.min(m, values[i]); return isFinite(m) ? m : 0
    }
    readonly property real _hi: {
        if (hi !== null && hi !== undefined) return hi
        var m = -Infinity; for (var i = 0; i < (values || []).length; i++) m = Math.max(m, values[i]); return isFinite(m) ? m : 0
    }

    Column {
        visible: chart.axis
        Repeater {
            model: chart.rows
            Text {
                required property int index
                height: chart.kit.cellH
                text: chart.kit.padL(index === 0 ? chart.format(chart._hi)
                                    : index === chart.rows - 1 ? chart.format(chart._lo) : "", chart.axisCells - 1) + " "
                color: chart.kit.dim; font: chart.kit.font; textFormat: Text.PlainText
            }
        }
    }
    Column {
        Repeater {
            model: chart.rows
            Text {
                required property int index
                height: chart.kit.cellH
                text: chart._lines[index] || ""
                color: chart.color; font: chart.kit.font; textFormat: Text.PlainText
            }
        }
    }
}
