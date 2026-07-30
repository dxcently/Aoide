// AoidePanel.qml — the center-left CODEX (surface: "dock").
//
// The temple's home, bound as a book. The four self-framed marble-stele gadgets
// — Conductor · Terminals · Meters · Power — no longer stack into one tall
// column that overflows the screen; they are the leaves of a FOLD-OUT JOURNAL
// pinned to the LEFT edge and vertically CENTRED. Summoned on SUPER+P, the codex
// unfolds open from its spine at the screen edge; flip through it a spread at a
// time. This is a change of PRESENTATION only — the wiring (shortcut/toggle,
// notes/bridge/shared plumbing, left-centre anchoring) is unchanged.
//
// ── Why GlobalShortcut, not the bridge ──────────────────────────────────────
// Same rationale as AoideLauncher: ShellBridge is OUTBOUND-only and there is no
// inbound CLI verb to toggle a surface. The codex registers a Hyprland global
// shortcut in-process and receives the keypress directly — the compositor binds
// `SUPER, P, global, aoide:dock`. One press summons, the next dismisses.
//
// ── The fold-out ────────────────────────────────────────────────────────────
// `shown` is the logical toggle; `fold` (0 closed .. 1 open) is the animated
// state it drives through a Behavior. The whole book is rotated around the Y
// axis with its ORIGIN AT THE LEFT EDGE (x:0) — the spine — so at fold 0 it is
// edge-on (a shut codex at the screen edge) and at fold 1 it lies flat open,
// ~230ms OutCubic. Because the close must be seen too, the PanelWindow stays
// `visible` until `fold` has eased back to nothing.
//
// ── Widget PAGES (pagination, not scroll) ───────────────────────────────────
// The book opens to a two-page SPREAD; the four gadgets are its faces:
//   spread 0 →  Conductor | Terminals      spread 1 →  Meters | Power
// A leaf-turn (a Y-rotation of the page layer about the central spine, with the
// content swapped while edge-on) carries you between spreads. Turn arrows ◁ ▷,
// a pair of note-dots, the mouse wheel, and the ← → keys all flip it. No tall
// stack, no scrollbar-scroll of the container.
//
// ── One normalized page ─────────────────────────────────────────────────────
// Every leaf is the SAME fixed size (`pageW` × `pageH`, itself capped so the
// whole codex is ≤ 92% of the screen). Each gadget is dropped into that leaf's
// inner frame: the tall rosters (Conductor/Terminals) FILL the frame and let
// their internal ListView clip/scroll WITHIN the page, so a page never exceeds
// the codex; the smaller gadgets (Meters/Power) take the page width and centre
// on it. Conductor/Terminals declare `bridge` + `shared` and get them;
// Meters/Power declare only `notes` (passing an undeclared property is an
// error), so they get only `notes`.
//
// Chrome: a marble codex in the pantheon key — an opaque cover with a hard plum
// border + inset gold keyline, a carved spine, page-edge striations, a clef
// cartouche header and turn controls. All colour flows from `notes`; radius 0
// everywhere.

import QtQuick
import Quickshell
import Quickshell.Wayland
import Quickshell.Hyprland

PanelWindow {
    id: root

    // ── Note + bridge + shared dependencies (injected by shell.qml) ─────────
    required property var notes
    required property var bridge
    property var shared: null

    // ── Visibility gate ────────────────────────────────────────────────────
    // `shown` is the logical toggle; `fold` is the animated open amount it
    // drives. The surface must remain `visible` while the fold-back plays out,
    // so visibility follows `shown OR still-folding`.
    property bool shown: false
    property real fold: 0            // 0 = shut spine · 1 = laid flat open
    Behavior on fold { NumberAnimation { duration: 230; easing.type: Easing.OutCubic } }

    function show()   { root.shown = true }
    function hide()   { root.shown = false }
    function toggle() { root.shown = !root.shown }

    onShownChanged: {
        root.fold = root.shown ? 1 : 0
        if (root.shown) {
            root.page = 0            // always open to the first spread
            turn.angle = 0
            root.flipping = false
            book.forceActiveFocus()  // best-effort: arrows/Esc when granted focus
        }
    }

    // ── Layer-shell surface (Overlay, left-edge, vertically centred) ────────
    // Anchored to the LEFT edge ONLY — top/bottom unanchored pins it left and
    // centres it on the vertical axis. exclusiveZone 0 reserves nothing;
    // transparent so only the codex draws.
    anchors { left: true }
    margins.left: 10
    exclusiveZone: 0
    color: "transparent"
    visible: root.shown || root.fold > 0.01

    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "aoide-dock"
    // OnDemand so a click into a page control can take focus, None at rest so
    // the shut codex never holds the keyboard.
    WlrLayershell.keyboardFocus: root.shown ? WlrKeyboardFocus.OnDemand
                                            : WlrKeyboardFocus.None

    // The window is sized to the OPEN codex at all times, so the fold-out has
    // room to render — the rotation only ever shrinks the drawn width.
    implicitWidth: book.width
    implicitHeight: book.height

    // ── Trigger: Hyprland global shortcut (aoide:dock) ─────────────────────
    GlobalShortcut {
        appid: "aoide"
        name: "dock"
        description: "Summon the Aoide center-left codex"
        onPressed: root.toggle()
    }

    // ── Palette helper (alpha over a role, as the gadgets do) ───────────────
    function withA(cstr, a) {
        var c = Qt.darker(cstr, 1.0);
        return Qt.rgba(c.r, c.g, c.b, a);
    }

    // type voices — shared with the temple gadgets ──────────────────────────
    readonly property string faceSerif: "Noto Serif"
    readonly property string faceMono:  "JetBrainsMono Nerd Font"
    readonly property string faceMusic: "Noto Music"

    // ── Geometry ────────────────────────────────────────────────────────────
    // One normalized page; the whole codex capped at ≤ 92% of the output.
    readonly property real screenH: root.screen ? root.screen.height : 1080
    readonly property real maxJournalH: screenH * 0.92
    readonly property int  coverPad: 16     // marble border zone around the leaves
    readonly property int  spineW:   30     // the carved central spine
    readonly property int  pageInset: 8     // leaf padding around a hosted gadget
    readonly property int  pageW: 376       // → inner frame 360, the gadgets' native width
    // Page height fills what's left under the ≤92% cap (header 30 + gaps 16 +
    // footer 34 + 2·coverPad 32 = 112 of chrome), never taller than a full roster.
    readonly property real pageH: Math.min(556, maxJournalH - 112)

    // ── Spread state + leaf-turn ────────────────────────────────────────────
    property int  page: 0
    readonly property int spreadCount: 2
    property bool flipping: false
    property int  flipDir: 1
    property int  pendingPage: 0
    readonly property var spreadTitle: ["𝄞  CONDUCTOR · TERMINALS",
                                        "𝄢  METERS · POWER"]

    function flipTo(t) {
        t = Math.max(0, Math.min(root.spreadCount - 1, t));
        if (t === root.page || root.flipping) return;
        root.flipDir = t > root.page ? 1 : -1;
        root.pendingPage = t;
        root.flipping = true;
        flipOut.start();
    }
    function nextSpread() { flipTo(root.page + 1) }
    function prevSpread() { flipTo(root.page - 1) }

    // outgoing: the page layer rotates to edge-on; at the seam the spread is
    // swapped and the layer jumps to the opposite edge for the incoming turn.
    NumberAnimation {
        id: flipOut
        target: turn; property: "angle"
        to: root.flipDir * -82; duration: 120; easing.type: Easing.InQuad
        onFinished: {
            root.page = root.pendingPage;
            turn.angle = root.flipDir * 82;
            flipIn.start();
        }
    }
    NumberAnimation {
        id: flipIn
        target: turn; property: "angle"
        to: 0; duration: 140; easing.type: Easing.OutQuad
        onFinished: root.flipping = false
    }

    // ── A page leaf: normalized frame that hosts one gadget ─────────────────
    component Leaf: Item {
        id: lf
        width: root.pageW
        height: root.pageH
        default property alias content: holder.data

        Rectangle {                              // the marble leaf
            anchors.fill: parent
            radius: 0
            color: root.withA(root.notes.paletteFg, 0.04)
            border.color: root.withA(root.notes.paletteFg, 0.16)
            border.width: 1
        }
        Item {                                   // inner frame the gadget lives in
            id: holder
            anchors.fill: parent
            anchors.margins: root.pageInset
        }
    }

    // ══ THE CODEX ═══════════════════════════════════════════════════════════
    Item {
        id: book
        width: 2 * root.pageW + root.spineW + 2 * root.coverPad
        height: root.pageH + 112
        focus: true

        // Fold-out: rotate about the LEFT edge (the spine) — edge-on when shut,
        // flat when open. Opacity leads slightly so the reveal reads.
        opacity: Math.min(1, root.fold * 1.35)
        transform: Rotation {
            origin.x: 0; origin.y: book.height / 2
            axis { x: 0; y: 1; z: 0 }
            angle: (1 - root.fold) * -90
        }

        // Keyboard flip (best-effort — granted only when the layer holds focus)
        Keys.onLeftPressed:  root.prevSpread()
        Keys.onRightPressed: root.nextSpread()
        Keys.onEscapePressed: root.hide()

        // cast shadow — lifts the codex off the marble wallpaper
        Rectangle {
            anchors.fill: cover
            anchors.leftMargin: 5; anchors.topMargin: 6
            anchors.rightMargin: -5; anchors.bottomMargin: -6
            radius: 0
            color: root.withA(root.notes.paletteFg, 0.22)
        }

        // the cover ──────────────────────────────────────────────────────────
        Rectangle {
            id: cover
            anchors.fill: parent
            radius: 0
            color: root.notes.paletteBg
            border.color: root.notes.paletteFg
            border.width: 2

            Rectangle {                          // inset gold keyline
                anchors.fill: parent; anchors.margins: 4
                radius: 0; color: "transparent"
                border.color: root.notes.paletteAccent; border.width: 1
            }

            // wheel-flip catcher — sits BEHIND the leaves, so scrolling over a
            // roster scrolls the roster and scrolling the margins turns the page.
            MouseArea {
                anchors.fill: parent
                acceptedButtons: Qt.NoButton
                onWheel: function (w) {
                    if (w.angleDelta.y < 0 || w.angleDelta.x > 0) root.nextSpread();
                    else if (w.angleDelta.y > 0 || w.angleDelta.x < 0) root.prevSpread();
                }
            }

            // ── HEADER: clef cartouche naming the current spread ─────────────
            Item {
                id: header
                anchors.top: parent.top; anchors.topMargin: root.coverPad
                anchors.left: parent.left; anchors.right: parent.right
                anchors.leftMargin: root.coverPad; anchors.rightMargin: root.coverPad
                height: 30

                Rectangle {                      // deeper marble band
                    anchors.fill: parent; anchors.bottomMargin: 4
                    color: root.withA(root.notes.paletteFg, 0.05)
                }
                Text {
                    anchors.centerIn: parent
                    anchors.verticalCenterOffset: -2
                    text: root.spreadTitle[root.page]
                    font.family: root.faceSerif
                    font.pixelSize: 14
                    font.letterSpacing: 3
                    color: root.notes.paletteAccent
                }
                // a hairline gold rule under the title
                Rectangle {
                    anchors.left: parent.left; anchors.right: parent.right
                    anchors.bottom: parent.bottom
                    height: 1
                    color: root.withA(root.notes.paletteAccent, 0.5)
                }
            }

            // ── PAGE AREA: two leaves either side of a static spine ──────────
            Item {
                id: pageArea
                anchors.top: header.bottom; anchors.topMargin: 8
                anchors.horizontalCenter: parent.horizontalCenter
                width: 2 * root.pageW + root.spineW
                height: root.pageH

                // outer page-edge striations — the sense of stacked leaves
                Row {
                    anchors.left: parent.left
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 2
                    Repeater {
                        model: 3
                        Rectangle {
                            width: 1; height: root.pageH - 8
                            color: root.withA(root.notes.paletteFg, 0.12 - index * 0.03)
                        }
                    }
                }
                Row {
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    layoutDirection: Qt.RightToLeft
                    spacing: 2
                    Repeater {
                        model: 3
                        Rectangle {
                            width: 1; height: root.pageH - 8
                            color: root.withA(root.notes.paletteFg, 0.12 - index * 0.03)
                        }
                    }
                }

                // ── the flipping leaf layer (both spreads live here) ─────────
                Item {
                    id: leafLayer
                    anchors.fill: parent
                    transform: Rotation {
                        id: turn
                        origin.x: pageArea.width / 2   // hinge on the spine
                        origin.y: pageArea.height / 2
                        axis { x: 0; y: 1; z: 0 }
                        angle: 0
                    }

                    // SPREAD 0 — Conductor | Terminals (roster temples: fill the leaf)
                    Row {
                        id: spreadA
                        anchors.fill: parent
                        spacing: root.spineW
                        visible: root.page === 0
                        Leaf {
                            ConductorGadget {
                                anchors.fill: parent
                                notes: root.notes
                                bridge: root.bridge
                                shared: root.shared
                            }
                        }
                        Leaf {
                            TerminalsGadget {
                                anchors.fill: parent
                                notes: root.notes
                                bridge: root.bridge
                                shared: root.shared
                            }
                        }
                    }

                    // SPREAD 1 — Meters | Power (compact temples: full width, centred)
                    Row {
                        id: spreadB
                        anchors.fill: parent
                        spacing: root.spineW
                        visible: root.page === 1
                        Leaf {
                            MetersGadget {
                                anchors.horizontalCenter: parent.horizontalCenter
                                anchors.verticalCenter: parent.verticalCenter
                                width: parent.width
                                notes: root.notes
                            }
                        }
                        Leaf {
                            PowerVitalsGadget {
                                anchors.horizontalCenter: parent.horizontalCenter
                                anchors.verticalCenter: parent.verticalCenter
                                width: parent.width
                                notes: root.notes
                            }
                        }
                    }
                }

                // ── the static SPINE, drawn over the centre gap ──────────────
                Rectangle {
                    id: spine
                    anchors.horizontalCenter: parent.horizontalCenter
                    anchors.top: parent.top; anchors.bottom: parent.bottom
                    width: root.spineW
                    color: root.withA(root.notes.paletteFg, 0.10)

                    Rectangle {                  // twin gold rules
                        anchors.left: parent.left; anchors.leftMargin: 5
                        anchors.top: parent.top; anchors.bottom: parent.bottom
                        width: 1; color: root.withA(root.notes.paletteAccent, 0.55)
                    }
                    Rectangle {
                        anchors.right: parent.right; anchors.rightMargin: 5
                        anchors.top: parent.top; anchors.bottom: parent.bottom
                        width: 1; color: root.withA(root.notes.paletteAccent, 0.55)
                    }
                    // stitched binding stations down the spine
                    Column {
                        anchors.centerIn: parent
                        spacing: 22
                        Repeater {
                            model: Math.max(3, Math.floor(root.pageH / 60))
                            Text {
                                text: "◆"
                                font.family: root.faceMusic
                                font.pixelSize: 8
                                color: root.withA(root.notes.paletteAccent, 0.7)
                            }
                        }
                    }
                }
            }

            // ── FOOTER: turn arrows + note-dot spread indicator ──────────────
            Item {
                id: footer
                anchors.bottom: parent.bottom; anchors.bottomMargin: root.coverPad
                anchors.left: parent.left; anchors.right: parent.right
                anchors.leftMargin: root.coverPad; anchors.rightMargin: root.coverPad
                height: 34

                Rectangle {                      // hairline gold rule above the nav
                    anchors.left: parent.left; anchors.right: parent.right
                    anchors.top: parent.top
                    height: 1
                    color: root.withA(root.notes.paletteAccent, 0.5)
                }

                Row {
                    anchors.centerIn: parent
                    spacing: 18

                    // ◁ previous spread
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        text: "◁"
                        font.family: root.faceSerif; font.pixelSize: 18
                        color: root.page > 0 ? root.notes.paletteAccent
                                             : root.withA(root.notes.paletteFg, 0.25)
                        opacity: prevMa.containsMouse ? 1.0 : 0.85
                        MouseArea {
                            id: prevMa
                            anchors.fill: parent; anchors.margins: -6
                            hoverEnabled: true
                            onClicked: root.prevSpread()
                        }
                    }

                    // ● ○  note-dots
                    Row {
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 10
                        Repeater {
                            model: root.spreadCount
                            Text {
                                text: index === root.page ? "♩" : "·"
                                font.family: root.faceMusic
                                font.pixelSize: index === root.page ? 15 : 18
                                color: index === root.page
                                       ? root.notes.paletteAccent
                                       : root.withA(root.notes.paletteFg, 0.4)
                                MouseArea {
                                    anchors.fill: parent; anchors.margins: -5
                                    onClicked: root.flipTo(index)
                                }
                            }
                        }
                    }

                    // ▷ next spread
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        text: "▷"
                        font.family: root.faceSerif; font.pixelSize: 18
                        color: root.page < root.spreadCount - 1
                               ? root.notes.paletteAccent
                               : root.withA(root.notes.paletteFg, 0.25)
                        opacity: nextMa.containsMouse ? 1.0 : 0.85
                        MouseArea {
                            id: nextMa
                            anchors.fill: parent; anchors.margins: -6
                            hoverEnabled: true
                            onClicked: root.nextSpread()
                        }
                    }
                }
            }
        }
    }
}
