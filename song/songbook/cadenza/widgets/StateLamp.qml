// StateLamp.qml — the session lamp: ● working · ◐ awaiting · ○ idle ·
// ■ stopped · ✕ failed. One cell wide.
//
// A HELPER (uppercase — never a slot), instantiated by URL
// (`kit.helper("StateLamp")`, design/kit.md §1). API: design/kit.md §StateLamp.
// Glyph and colour both come from the kit (`kit.lampGlyph(s)`,
// `kit.lampColor(s)`) — a list delegate calls those directly in a plain Text
// rather than loading this file per row; both agree by construction.
// `hot: true` repaints it amber — the ONE traced/live element (intent §2:
// never two amber things at rest); the caller owns keeping it to one.
// An unknown state draws a dim `·`, never a guess.
import QtQuick

Text {
    id: lamp

    required property var kit
    property string status: "idle"
    property bool hot: false

    width: kit.cellW
    text: kit.lampGlyph(status)
    color: hot ? kit.hot : kit.lampColor(status)
    font: kit.font
    textFormat: Text.PlainText
    horizontalAlignment: Text.AlignHCenter
}
