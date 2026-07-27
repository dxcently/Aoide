// GadgetFrame.qml — one Pantheon PANE (round 4 — the pane replaces the box).
//
// Round 4 retired the Windows-7 ASCII double-box chrome (╔═[ TITLE ]═╗ … ╚═╝).
// A gadget is now a PANTHEON PANE: a thin hollow WIREFRAME outline (base0C
// wireCyan, dim) around the SAME translucent Aero glass body + gloss (the glass
// stays — blur, sheen and frost are untouched), with the title rendered as a
// small lowercase dotted CALLOUT label sitting on the pane's top edge, tapped by
// a short angled LEADER line — the reference stills' `optic nerve.LE.dk.002`
// token pattern. All colors from notes (zero hardcoded hex; the one literal is
// the transparent fill of the outline/depth copies).
//
// Usage (default property → children land in the body):
//   GadgetFrame { notes: notes; title: "baton.control"; BatonGadget { … } }
//
// ── Vanishing-point depth (round 4) ─────────────────────────────────────────
// The two hollow back-outline copies no longer offset a fixed down-right: they
// project toward the SCREEN CENTRE (the refs' vanishing point at 960,540). Each
// host sets `depthDx`/`depthDy` — a normalized -1..1 direction — and the frame
// multiplies the offset magnitudes by it, so a pane on the left edge stacks
// rightward, a pane below-centre stacks up, etc. Default +1/+1 (down-right)
// suits the top-left field. Magnitudes are unchanged from round 3 — subtle by
// design. A clipping host must add `depthExtent` of headroom on the side the
// direction points (recheck when flipping a sign — it changes which side clips).

import QtQuick

Item {
    id: root

    required property var notes
    property string title: ""

    // ── Tear-off affordance (Win7 "drag gadget to the desktop") ─────────────
    // A small dim ↗ token folded into the callout row's tail; a transparent
    // MouseArea over it emits floatRequested(). The dock wires this to
    // DesktopGadgets; BarPopout leaves it false (no tear-off).
    property bool floatable: false
    signal floatRequested()

    // ── Chrome recede on trace (round 3, carried forward) ───────────────────
    // While a session is traced (the one-neon element blazes green in the body),
    // the pane's own chrome (callout, leader, outline, depth copies) steps back
    // to chromeOpacity so nothing competes with the hot element. The glass body
    // is left alone. The frame DISCOVERS the trace from its body child's `shared`
    // object; frames whose gadget is not trace-aware are unaffected. Overridable.
    property bool chromeDim: {
        var items = body.data
        for (var i = 0; i < items.length; i++) {
            var it = items[i]
            if (it && it.shared && it.shared.tracedSessionId !== undefined
                    && it.shared.tracedSessionId !== "")
                return true
        }
        return false
    }
    readonly property real chromeOpacity: chromeDim ? 0.55 : 1.0

    // ── Glass tuning ────────────────────────────────────────────────────────
    property real glassOpacity: 0.72

    // ── Pantheon wireframe-depth seam (tunable constants) ───────────────────
    // Two hollow OUTLINE copies of the pane, offset toward the vanishing point
    // behind the glass, at decreasing opacity — the "stacked offset volume" read
    // from the reference stills. Transparent fill + border only (no MouseArea)
    // so they never intercept input. Back copies are holoBlue (base0D) — a step
    // cooler/dimmer than the wireCyan front outline, so the stack reads as depth.
    property int depthOff1: 3        // near copy offset magnitude (px)
    property int depthOff2: 6        // far copy offset magnitude (px)
    property real depthOpacity1: 0.35
    property real depthOpacity2: 0.18
    readonly property int depthExtent: depthOff2   // headroom a clipper must add

    // Vanishing-point direction — normalized -1..1; +1/+1 = down-right (default,
    // suits the top-left field). Hosts override per their position vs (960,540).
    property real depthDx: 1
    property real depthDy: 1

    // ── Body content sink (children nest here) ──────────────────────────────
    default property alias content: body.data

    implicitHeight: frameColumn.implicitHeight + 14

    // ── Wireframe depth stack (declared first → renders behind the glass) ───
    // Far copy (dimmer, ×off2) then near copy (×off1), each projected along the
    // vanishing-point direction. Only the sliver past the pane edge shows as a
    // clean outline; the rest reads as a faint double-rule ghost through glass.
    Rectangle {
        x: root.depthOff2 * root.depthDx; y: root.depthOff2 * root.depthDy
        width: root.width; height: root.height
        radius: 4
        color: "transparent"
        border.color: notes.holoBlue
        border.width: 1
        opacity: root.depthOpacity2 * root.chromeOpacity
    }
    Rectangle {
        x: root.depthOff1 * root.depthDx; y: root.depthOff1 * root.depthDy
        width: root.width; height: root.height
        radius: 4
        color: "transparent"
        border.color: notes.holoBlue
        border.width: 1
        opacity: root.depthOpacity1 * root.chromeOpacity
    }

    // ── Glass panel (unchanged — blur/frost/translucency stays) ─────────────
    Rectangle {
        anchors.fill: parent
        radius: 4
        color: notes.paletteBg
        opacity: root.glassOpacity      // translucency → glass over blur
    }

    // Gloss — the Aero sheen (bright top half, hard midline stop, faint bloom).
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

    // ── Wireframe outline (the front face — crisp hollow rule over the glass) ─
    Rectangle {
        anchors.fill: parent
        radius: 4
        color: "transparent"
        border.color: notes.wireCyan
        border.width: 1
        opacity: 0.5 * root.chromeOpacity
    }

    Column {
        id: frameColumn
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.margins: 6
        spacing: 3

        // ── Callout title — lowercase dotted label on a short angled leader ──
        Item {
            width: parent.width
            implicitHeight: Math.max(calloutText.implicitHeight, 12)

            Row {
                id: calloutRow
                anchors.left: parent.left
                anchors.verticalCenter: parent.verticalCenter
                spacing: 4

                // Leader — the refs' anchor tick + short rule tapping the label.
                Canvas {
                    id: leader
                    anchors.verticalCenter: parent.verticalCenter
                    width: 13
                    height: 10
                    opacity: 0.6 * root.chromeOpacity
                    onPaint: {
                        var ctx = getContext("2d")
                        ctx.reset()
                        ctx.clearRect(0, 0, width, height)
                        ctx.strokeStyle = root.notes.wireCyan
                        ctx.fillStyle = root.notes.wireCyan
                        ctx.lineWidth = 1
                        var cy = height / 2
                        // anchor tick at the left, a horizontal rule, a terminal dot
                        ctx.beginPath(); ctx.moveTo(1, cy - 3); ctx.lineTo(1, cy + 3); ctx.stroke()
                        ctx.beginPath(); ctx.moveTo(1, cy); ctx.lineTo(width - 2, cy); ctx.stroke()
                        ctx.beginPath(); ctx.arc(width - 2, cy, 1.3, 0, 2 * Math.PI); ctx.fill()
                    }
                    Connections {
                        target: root.notes
                        function onWireCyanChanged() { leader.requestPaint() }
                    }
                }

                Text {
                    id: calloutText
                    anchors.verticalCenter: parent.verticalCenter
                    text: root.title
                    color: notes.paletteFg
                    opacity: 0.55 * root.chromeOpacity
                    font.family: "monospace"
                    font.pixelSize: 11
                    elide: Text.ElideRight
                }
            }

            // Tear-off token — dim ↗ at the callout's tail (affordance, not song).
            Text {
                id: floatToken
                visible: root.floatable
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                text: "↗"
                color: notes.wireCyan
                opacity: 0.6 * root.chromeOpacity
                font.family: "monospace"
                font.pixelSize: 12
            }
        }

        // ── Body (children nest here via default alias) ──────────────────
        Item {
            id: body
            width: parent.width
            implicitHeight: childrenRect.height
        }
    }

    // ── Tear-off click target — over the ↗ token at the callout's right end ──
    MouseArea {
        visible: root.floatable
        enabled: root.floatable
        width: 22
        height: 22
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.rightMargin: 3
        anchors.topMargin: 3
        cursorShape: Qt.PointingHandCursor
        onClicked: root.floatRequested()
    }
}
