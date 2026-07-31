// GadgetFrame.qml — one sonata TEMPLE BAY (Greek-grammar reskin of the pane).
//
// Sonata's house grammar (song/songbook/sonata/design/greek-grammar.md §4) recasts
// the former Pantheon wireframe pane as a FLAT typographic temple bay: an
// entablature drawn from box characters over the SAME translucent Aero glass body
// + gloss (glass untouched — blur, sheen, frost stay). The 3D vanishing-point
// depth stack is RETIRED (this grammar is flat). Element map:
//   • pediment  — a ╱‾‾ ╲ raking cap carrying the surface's Greek order-mark at
//                 the apex; the orchestration token is inscribed on the architrave
//                 rule ─── beneath it (labelColor = paletteFg, same dim).
//   • cornice   — a heavy ═══ rule under the pediment.
//   • columns   — ║ side rails down the left/right body edges (1px verdigris rule;
//                 a tiled ║ glyph breaks up at variable bay heights, so the shaft
//                 is drawn as a crisp rule per the LEGIBILITY-FIRST rule).
//   • stylobate — a ▔▁ stepped base course the whole bay stands on.
// All structural chrome wears the wireCyan (bronze-verdigris) role at ≤ 0.5 alpha
// — a HARD invariant (grammar §5): on warm marble the verdigris out-reads the
// laurel one-hot blaze unless the opacity ladder caps it. radius: 0 kept.
// All colors from notes (zero hardcoded hex).
//
// Usage (default property → children land in the body):
//   GadgetFrame { notes: notes; title: "conductor.control"; ConductorGadget { … } }

import QtQuick

Item {
    id: root

    required property var notes
    property string title: ""

    // ── Greek order-mark (grammar §1) — the pediment-apex label per surface ──
    // A stable per-surface capital-Greek order-mark; the orchestration token
    // (title) survives verbatim as the architrave inscription beneath it, so
    // identity/search are unaffected. Unknown surfaces fall back to the neutral
    // middot (a music-contract glyph, harmless).
    readonly property var orderMarks: ({
        "conductor.control": "Α",  // Α
        "terminals.roster":  "Β",  // Β
        "dag.trace":         "Γ",  // Γ
        "meters.pulse":      "Δ",  // Δ
        "power.reserve":     "Θ",  // Θ
        "volume.level":      "Λ",  // Λ
        "audio.control":     "Η",  // Η — reads as two joined columns (the porch)

        "battery.gauge":     "Ξ",  // Ξ
        "calendar.sheet":    "Π",  // Π
        "nowplaying.score":  "Σ",  // Σ
        "gadgets.case":      "Ω"   // Ω
    })
    readonly property string orderMark:
        orderMarks[title] !== undefined ? orderMarks[title] : "·"

    // ── Tear-off affordance (drag gadget to the desktop) ────────────────────
    // A small dim ↗ token folded into the pediment tail; a transparent MouseArea
    // over it emits floatRequested(). The dock wires this to DesktopGadgets;
    // BarPopout leaves it false (no tear-off).
    property bool floatable: false
    signal floatRequested()

    // ── Chrome recede on trace (grammar §4) ─────────────────────────────────
    // While a session is traced (the one laurel blaze fires in the body), the
    // bay's own chrome (pediment, columns, cornice, stylobate) steps back to
    // chromeOpacity so nothing competes with the hot element. The glass body is
    // left alone. The frame DISCOVERS the trace from its body child's `shared`
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

    // ── Chrome colour overrides (additive; default to the song's note reads) ──
    // The dock leaves these at their defaults, so its bays render unchanged. The
    // bar's BarPopout overrides them to the popout palette (opaque glass, ink
    // chrome/label) so the bar's popouts match the sheet-music bar.
    property color glassColor: notes.paletteBg
    property color outlineColor: notes.wireCyan     // verdigris structural chrome
    property color labelColor: notes.paletteFg

    // ── Retired depth seam (kept as no-op declarations for API compatibility) ─
    // The flat Greek grammar drops the two hollow vanishing-point outline copies.
    // These properties are no longer drawn, but hosts (BarPopout, DesktopGadgets)
    // still SET some of them, so the declarations remain to keep the public API
    // intact. depthExtent is now 0 — the flat bay needs no offset headroom.
    property color depthColor: notes.holoBlue       // unused (was back-copy hue)
    property int depthOff1: 3                        // unused
    property int depthOff2: 6                        // unused
    property real depthOpacity1: 0.35                // unused
    property real depthOpacity2: 0.18                // unused
    readonly property int depthExtent: 0             // flat grammar — no headroom
    property real depthDx: 1                          // unused (no lean)
    property real depthDy: 1                          // unused (no lean)

    // ── Body content sink (children nest here) ──────────────────────────────
    default property alias content: body.data

    implicitHeight: frameColumn.implicitHeight + 14

    // ── Glass panel (unchanged — blur/frost/translucency stays) ─────────────
    Rectangle {
        anchors.fill: parent
        radius: 0
        color: root.glassColor
        opacity: root.glassOpacity      // translucency → glass over blur
    }

    // Gloss — the Aero sheen (bright top half, hard midline stop, faint bloom).
    Rectangle {
        anchors.fill: parent
        radius: 0
        gradient: Gradient {
            GradientStop { position: 0.0;  color: Qt.rgba(1, 1, 1, 0.14) }
            GradientStop { position: 0.42; color: Qt.rgba(1, 1, 1, 0.04) }
            GradientStop { position: 0.5;  color: Qt.rgba(1, 1, 1, 0.00) }
            GradientStop { position: 1.0;  color: Qt.rgba(1, 1, 1, 0.04) }
        }
    }

    // ── Column rails — the ║ side order down the left/right body edges ───────
    // A crisp 1px verdigris shaft each side; wireCyan structural role, ≤ 0.5.
    Rectangle {
        anchors.left: parent.left
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        width: 1
        color: root.outlineColor
        opacity: 0.5 * root.chromeOpacity
    }
    Rectangle {
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        width: 1
        color: root.outlineColor
        opacity: 0.5 * root.chromeOpacity
    }

    // ── Stylobate — the stepped base course the bay stands on (▔ over ▁) ──────
    Column {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        spacing: 0
        Text {
            width: parent.width
            clip: true
            text: "▔".repeat(120)        // ▔ upper step edge
            color: root.outlineColor
            opacity: 0.5 * root.chromeOpacity
            font.family: "monospace"
            font.pixelSize: 8
        }
        Text {
            width: parent.width
            clip: true
            text: "▁".repeat(120)        // ▁ ground line
            color: root.outlineColor
            opacity: 0.5 * root.chromeOpacity
            font.family: "monospace"
            font.pixelSize: 8
        }
    }

    Column {
        id: frameColumn
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.margins: 6
        spacing: 2

        // ── Pediment header — raking cap + cornice + inscribed architrave ────
        Item {
            width: parent.width
            implicitHeight: pediment.implicitHeight

            Column {
                id: pediment
                width: parent.width
                spacing: 0

                // Raking cornice cap ╱‾‾ Α ‾‾╲ — order-mark at the apex.
                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: "╱‾‾ " + root.orderMark + " ‾‾╲"
                    color: root.outlineColor
                    opacity: 0.5 * root.chromeOpacity
                    font.family: "monospace"
                    font.pixelSize: 10
                }

                // Cornice — the heavy rule under the pediment.
                Text {
                    width: parent.width
                    clip: true
                    text: "═".repeat(120)        // ═══
                    color: root.outlineColor
                    opacity: 0.5 * root.chromeOpacity
                    font.family: "monospace"
                    font.pixelSize: 9
                }

                // Architrave — the plain rule the inscription sits on. The short
                // ─── tick (replacing the old anchor-tick leader) is followed by
                // the orchestration token in labelColor; the ↗ tear-off token
                // folds into the tail.
                Item {
                    width: parent.width
                    implicitHeight: Math.max(inscription.implicitHeight, 12)

                    Row {
                        anchors.left: parent.left
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 4

                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            text: "───"    // ───
                            color: root.outlineColor
                            opacity: 0.5 * root.chromeOpacity
                            font.family: "monospace"
                            font.pixelSize: 11
                        }

                        Text {
                            id: inscription
                            anchors.verticalCenter: parent.verticalCenter
                            text: root.title
                            color: root.labelColor
                            opacity: 0.55 * root.chromeOpacity
                            font.family: "monospace"
                            font.pixelSize: 11
                            elide: Text.ElideRight
                        }
                    }

                    // Tear-off token — dim ↗ folded into the pediment tail.
                    Text {
                        id: floatToken
                        visible: root.floatable
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        text: "↗"                    // ↗
                        color: root.outlineColor
                        opacity: 0.6 * root.chromeOpacity
                        font.family: "monospace"
                        font.pixelSize: 12
                    }
                }
            }
        }

        // ── Body (children nest here via default alias) ──────────────────
        Item {
            id: body
            width: parent.width
            implicitHeight: childrenRect.height
        }
    }

    // ── Tear-off click target — over the ↗ token at the pediment's right end ──
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
