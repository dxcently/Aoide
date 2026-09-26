// Gauge.qml — termui's gauge: eighth-blocks `█▉▊▋▌▍▎▏` on a `░` track.
//
//     CPU ███████▍░░░░░░░░  46%
//
// A HELPER (uppercase — never a slot), instantiated by URL
// (`kit.helper("Gauge")`, design/kit.md §1). API: design/kit.md §Gauge.
// Layout on the cell grid: [label padded to `labelCells`] [track `cells`]
// [pct right-aligned in 5 cells]. The lit run is `color` (default chart ink),
// the track `kit.dim`. `warnAt`/`urgentAt` (0..1, or -1 off) repaint the lit
// run orange / red past a threshold — `invert: true` flips that for a gauge
// where LOW is the trouble (battery).
import QtQuick

Row {
    id: gauge

    required property var kit
    property real value: 0              // 0..1
    property int cells: 16              // track width
    property string label: ""
    property int labelCells: label.length ? label.length + 1 : 0
    property bool showPct: true
    property string valueText: Math.round(Math.max(0, Math.min(1, value)) * 100) + "%"
    property color color: kit.ink
    property real warnAt: -1
    property real urgentAt: -1
    property bool invert: false

    function _past(t) { return t >= 0 && (invert ? value <= t : value >= t) }
    readonly property color litColor: _past(urgentAt) ? kit.urgent : _past(warnAt) ? kit.warn : color
    readonly property var _run: kit.gaugeText(value, cells)

    spacing: 0
    Text {
        visible: gauge.labelCells > 0
        text: gauge.kit.padR(gauge.label, gauge.labelCells)
        color: gauge.kit.dim; font: gauge.kit.font; textFormat: Text.PlainText
    }
    Text { text: gauge._run.fill;  color: gauge.litColor; font: gauge.kit.font; textFormat: Text.PlainText }
    Text { text: gauge._run.track; color: gauge.kit.dim;  font: gauge.kit.font; textFormat: Text.PlainText }
    Text {
        visible: gauge.showPct
        text: gauge.kit.padL(gauge.valueText, 5)
        color: gauge.kit.number; font: gauge.kit.font; textFormat: Text.PlainText
    }
}
