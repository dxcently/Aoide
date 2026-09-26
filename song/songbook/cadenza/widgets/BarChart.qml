// BarChart.qml — termui's bar chart: full-block columns, value on the base.
//
//     █
//     █  ▆
//     █  █  ▃
//    4.2 1.9 .8     value (cyan)
//    [2] [3] [5]    label (blue — a workspace / project name)
//
// A HELPER (uppercase — never a slot), instantiated by URL
// (`kit.helper("BarChart")`, design/kit.md §1). API: design/kit.md §BarChart.
// `bars`: [{ label, value, color? }]. Each column is `barCells` wide with
// `gapCells` between; `rows` lines tall with an eighth-block cap. Scale is
// `max` (or the largest value when `max` <= 0).
import QtQuick

Row {
    id: chart

    required property var kit
    property var bars: []
    property int rows: 5
    property int barCells: 3
    property int gapCells: 1
    property real max: 0
    property var format: function(v) { return v >= 1000 ? (v / 1000).toFixed(1) + "k" : String(Math.round(v * 10) / 10) }
    property color color: kit.ink
    property color labelColor: kit.path

    readonly property real _peak: {
        if (max > 0) return max
        var m = 0; for (var i = 0; i < (bars || []).length; i++) m = Math.max(m, bars[i].value || 0); return m
    }
    spacing: kit.cells(gapCells)

    Repeater {
        model: chart.bars
        Column {
            required property var modelData
            readonly property var _col: chart.kit.barColumn(chart._peak > 0 ? (modelData.value || 0) / chart._peak : 0, chart.rows)
            Repeater {
                model: chart.rows
                Text {
                    required property int index
                    height: chart.kit.cellH
                    text: { var g = parent._col ? parent._col[index] : " "; var s = ""; for (var i = 0; i < chart.barCells; i++) s += g; return s }
                    color: parent.modelData && parent.modelData.color ? parent.modelData.color : chart.color
                    font: chart.kit.font; textFormat: Text.PlainText
                }
            }
            Text {
                text: chart.kit.padR(chart.format(modelData.value || 0), chart.barCells)
                color: chart.kit.number; font: chart.kit.font; textFormat: Text.PlainText
            }
            Text {
                text: chart.kit.padR(String(modelData.label), chart.barCells)
                color: chart.labelColor; font: chart.kit.font; textFormat: Text.PlainText
            }
        }
    }
}
