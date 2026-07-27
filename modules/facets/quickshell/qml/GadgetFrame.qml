// GadgetFrame.qml — one Win7-gadget frame with box-drawing (ASCII) chrome.
//
// Wraps a single gadget's content on an Aero-glass panel: a semi-transparent
// paletteBg Rectangle (reads as frosted glass over the compositor's Hyprland
// blur) bordered in paletteAccent, topped by an ASCII title bar rendered with
// double box-drawing characters:  ╔═[ TITLE ]═╗ … ║ body ║ … ╚═══════════╝.
// All colors from notes (zero hardcoded hex); the chrome is monospace.
//
// Usage (default property → children land in the body):
//   GadgetFrame { notes: notes; title: "CLOCK"; ClockGadget { notes: notes } }
//
// The title bar's fill dashes are sized to the panel width in JS so the ╗ always
// lands at the right edge (machine-checked in node — see the task report).

import QtQuick

Item {
    id: root

    required property var notes
    property string title: ""

    // ── Tear-off affordance (Win7 "drag gadget to the desktop") ─────────────
    // When `floatable`, a small [↗] token is folded into the title line's tail
    // (exactly the pin idiom from AoideAgentWidgets — an in-chrome glyph, not a
    // free-floating button, so the computed ═ fill stays alignment-exact) and a
    // transparent MouseArea over the right end emits floatRequested(). The dock
    // wires this to DesktopGadgets; BarPopout leaves it false (no tear-off).
    property bool floatable: false
    signal floatRequested()

    // ── Glass tuning ────────────────────────────────────────────────────────
    property real glassOpacity: 0.72
    readonly property int chromePx: 12      // monospace cell size for the chrome
    readonly property real cellW: chromePx * 0.6  // approx monospace advance

    // ── Body content sink (children nest here) ──────────────────────────────
    default property alias content: body.data

    implicitHeight: frameColumn.implicitHeight + 16

    // ── ASCII title line: ╔═[ TITLE ]═╗ padded to the panel width ──────────
    // segments: "╔═[ " + title + " ]" then fill "═" then "╗". Guard against a
    // title longer than the available run (fill clamps to >= 0).
    function titleLine(cols) {
        var head = "╔═[ " + root.title + " ]"
        var tail = root.floatable ? "[↗]╗" : "╗"
        var fillCount = cols - head.length - tail.length
        if (fillCount < 0) fillCount = 0
        var fill = ""
        for (var i = 0; i < fillCount; i++) fill += "═"
        return head + fill + tail
    }
    function footerLine(cols) {
        var c = cols - 2
        if (c < 0) c = 0
        var mid = ""
        for (var i = 0; i < c; i++) mid += "═"
        return "╚" + mid + "╝"
    }
    // How many monospace cells fit across the panel interior.
    readonly property int cols: Math.max(8, Math.floor(width / cellW))

    // ── Glass panel ─────────────────────────────────────────────────────────
    Rectangle {
        anchors.fill: parent
        radius: 4
        color: notes.paletteBg
        opacity: root.glassOpacity      // translucency → glass over blur
        border.color: notes.paletteAccent
        border.width: 1
    }

    // Gloss — the Aero sheen (same CSS-trick gradient as the bar strip):
    // bright top half, hard stop at the midline, faint bloom at the bottom.
    Rectangle {
        anchors.fill: parent
        radius: 4
        gradient: Gradient {
            GradientStop { position: 0.0;  color: Qt.rgba(1, 1, 1, 0.14) }
            GradientStop { position: 0.42; color: Qt.rgba(1, 1, 1, 0.04) }
            GradientStop { position: 0.5;  color: Qt.rgba(1, 1, 1, 0.00) }
            GradientStop { position: 1.0;  color: Qt.rgba(1, 1, 1, 0.04) }
        }
    }

    Column {
        id: frameColumn
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.margins: 4
        spacing: 3

        // ── ASCII title bar ──────────────────────────────────────────────
        Text {
            width: parent.width
            text: root.titleLine(root.cols)
            color: notes.paletteAccent
            font.family: "monospace"
            font.pixelSize: root.chromePx
            font.bold: true
            clip: true
        }

        // ── Body (children nest here via default alias) ──────────────────
        Item {
            id: body
            width: parent.width
            implicitHeight: childrenRect.height
        }

        // ── Footer ornament: clef-tail end-cap (ornament vocabulary) ─────
        // Placed INSIDE the frame, right-aligned above the footer rule: the
        // staff/clef glyphs are wide SMP characters whose cell width doesn't
        // match the ═ fill math, so they decorate the interior rather than
        // the computed box border (which must stay alignment-exact).
        Text {
            anchors.right: parent.right
            anchors.rightMargin: Math.round(root.cellW)
            text: "ৎ𝄢"
            color: notes.paletteAccent
            opacity: 0.55
            font.family: "monospace"
            font.pixelSize: root.chromePx
        }

        // ── ASCII footer ─────────────────────────────────────────────────
        Text {
            width: parent.width
            text: root.footerLine(root.cols)
            color: notes.paletteAccent
            opacity: 0.7
            font.family: "monospace"
            font.pixelSize: root.chromePx
            clip: true
        }
    }

    // ── Tear-off click target ─────────────────────────────────────────────
    // Sits over the [↗] token at the right end of the title line. Declared
    // AFTER frameColumn so it stacks above the title Text and takes the click.
    MouseArea {
        visible: root.floatable
        enabled: root.floatable
        width: 4 * root.cellW
        height: root.chromePx + 8
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.rightMargin: 4
        anchors.topMargin: 4
        cursorShape: Qt.PointingHandCursor
        onClicked: root.floatRequested()
    }
}
