// GlowText.qml — phosphor text with the glow budget of intent §2.
//
// A HELPER (uppercase — never a slot), instantiated by URL
// (`kit.helper("GlowText")`, design/kit.md §1). API: design/kit.md §GlowText.
//
//   glow: "off"      plain Text.
//   glow: "outline"  TIER 0 — the text's own `style: Text.Outline`,
//                    `styleColor` its colour at 0.18. The outline is drawn
//                    under the glyph fill in the same glyph run, so this IS
//                    the spec's "static second copy under it" with no second
//                    item: no offscreen buffer, no shader.
//   glow: "bloom"    TIER 0 + TIER 1 — adds a blurred copy under the text: a
//                    hidden source Text fed to one MultiEffect (blur only).
//                    MultiEffect renders its source through a
//                    ShaderEffectSource that re-renders only when the source
//                    changes, so a static title is blurred ONCE and reused.
//                    TITLES ONLY — never a list delegate, never an animating
//                    item (the measured budget, intent §2).
//
// Always Text.PlainText: a GlowText may carry untrusted words (house rule 4).
import QtQuick
import QtQuick.Effects

Item {
    id: g

    required property var kit
    property string text: ""
    property color color: kit.ink
    property bool bold: false
    property string glow: "outline"
    property real bloomAlpha: 0.55
    property real bloomBlur: 0.5          // MultiEffect.blur, 0..1 of blurMax
    property int bloomMax: 12             // px

    implicitWidth: sharp.implicitWidth
    implicitHeight: sharp.implicitHeight

    Loader {
        active: g.glow === "bloom" && g.text.length > 0
        anchors.fill: sharp
        sourceComponent: Item {
            Text {
                id: src
                visible: false
                text: g.text
                color: g.color
                font: g.bold ? g.kit.titleFont : g.kit.font
                textFormat: Text.PlainText
            }
            MultiEffect {
                source: src
                anchors.fill: src
                autoPaddingEnabled: true
                blurEnabled: true
                blur: g.bloomBlur
                blurMax: g.bloomMax
                opacity: g.bloomAlpha
            }
        }
    }

    Text {
        id: sharp
        text: g.text
        color: g.color
        font: g.bold ? g.kit.titleFont : g.kit.font
        textFormat: Text.PlainText
        style: g.glow === "off" ? Text.Normal : Text.Outline
        styleColor: g.kit.withA(g.color, 0.18)
    }
}
