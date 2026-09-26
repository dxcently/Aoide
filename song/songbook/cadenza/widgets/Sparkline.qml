// Sparkline.qml — `▁▂▃▄▅▆▇█`, one column per sample, newest on the right.
//
//     tok/min ▁▁▂▃▅▇█▆▄▃▂▁▁▂▃   1.2k
//
// A HELPER (uppercase — never a slot), instantiated by URL
// (`kit.helper("Sparkline")`, design/kit.md §1). API: design/kit.md §Sparkline.
// `values` is any number array; the last `cells` samples show, scaled to
// `max` (or the window's own peak when `max` <= 0). Optional dim `label`
// before, optional cyan `valueText` after.
import QtQuick

Row {
    id: spark

    required property var kit
    property var values: []
    property int cells: 16
    property real max: 0
    property string label: ""
    property int labelCells: label.length ? label.length + 1 : 0
    property string valueText: ""
    property color color: kit.ink

    Text {
        visible: spark.labelCells > 0
        text: spark.kit.padR(spark.label, spark.labelCells)
        color: spark.kit.dim; font: spark.kit.font; textFormat: Text.PlainText
    }
    Text {
        text: spark.kit.sparkText(spark.values, spark.cells, spark.max)
        color: spark.color; font: spark.kit.font; textFormat: Text.PlainText
    }
    Text {
        visible: spark.valueText.length > 0
        text: " " + spark.valueText
        color: spark.kit.number; font: spark.kit.font; textFormat: Text.PlainText
    }
}
