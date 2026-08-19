// AoidePanel.qml — the center-left DOCK (surface: "dock").
//
// A left-edge SLIDE-OUT dock whose container reads as the SIDE OF A BOOK peeking
// out at the screen edge. The four self-framed marble-stele gadgets — Conductor ·
// Terminals · Meters · Power — are stacked vertically at their NATURAL sizes
// inside a scrolling body; the chrome around them is a closed codex seen edge-on:
// a marble board with the hard plum border + inset gold keyline, a carved spine
// down the inner edge, and a stack of page-edge striations (the fore-edge) down
// the OUTER, screen-facing edge. This REPLACES the fold-out journal presentation
// (preserved separately as AoideJournal.qml); no fold, no spread, no rotation —
// just a horizontal slide.
//
// ── Why GlobalShortcut, not the bridge ──────────────────────────────────────
// Same rationale as AoideLauncher/the old dock: ShellBridge is OUTBOUND-only and
// there is no inbound CLI verb to toggle a surface. The dock registers a Hyprland
// global shortcut in-process and receives the keypress directly — the compositor
// binds `SUPER, P, global, aoide:dock`. One press pins it out, the next dismisses.
//
// ── The slide + the peeking fore-edge ───────────────────────────────────────
// The window is anchored to the LEFT edge only (top/bottom unanchored → pinned
// left, vertically CENTRED) and stays mapped at all times so the fore-edge is
// always visible and the hot strip is always live. `reveal` (0 shut .. 1 out) is
// the animated slide amount; the book translates X by -(bookW - sliverW)·(1-reveal)
// so that at rest only the rightmost `sliverW` — the fore-edge striations — peeks
// past the screen edge, and at full reveal the whole codex sits on-screen with its
// spine at x=0. ~200ms OutCubic. The INPUT MASK follows the slide (a live-tracked
// Region.item), so the shut dock only catches the sliver + hot strip and clicks
// elsewhere fall straight through; exclusiveZone 0 reserves nothing.
//
// `reveal` is driven by `dockOpen = shown || hotEdge || overPanel`:
//   · shown    — the pinned toggle (SUPER+P / show()/hide()/toggle()); stays out.
//   · hotEdge  — pointer inside the thin left hot strip (hover-peek).
//   · overPanel— pointer anywhere over the drawn codex (keeps a peek open).
// A ~450ms auto-hide grace after the hover union drops keeps it from flapping;
// pinning (shown) ignores the grace and holds it out until toggled shut.
//
// ── Scroll, not pagination ──────────────────────────────────────────────────
// The gadgets stack in a Flickable at ONE shared width (root.gadgetW = 360) so the
// column is flush — Conductor 360×520, Usage 360×content, Terminals 360×520,
// Meters 360×268, Power 360×268, all sharing a single left edge (the compact two
// no longer centred/inset). The body is capped at ≤92% of the screen and scrolls
// past it, with a slim gold scrollbar in the inner gutter (ScrollRail — drag the
// thumb, or press the track to jump) and a "▽ more" hint that fades at the end —
// a visible affordance that there is more below the fold. The fore-edge takes the
// wheel too, so the peeking page-block doubles as a scroll rail.
// Conductor/Terminals declare `bridge` + `shared` and get them; Meters/Power
// declare only `livery` (passing an undeclared property is an error), so they get
// only `livery`.
//
// All colour flows from `livery` roles; radius 0 everywhere.

import QtQuick
import Quickshell
import Quickshell.Wayland
import Quickshell.Hyprland
import Quickshell.Io

PanelWindow {
    id: root

    // ── Note + bridge + shared dependencies (injected by shell.qml) ─────────
    required property var livery
    required property var bridge
    property var shared: null
    // the song-slot resolver — only the herald-center WidgetSlot below needs
    // it; gadgets that are pure facet chrome never see it
    property var stagingEngine: null

    // ── Reveal state ────────────────────────────────────────────────────────
    // `shown` is the pinned toggle; `hotEdge`/`overPanel` are the hover peeks.
    // `dockOpen` is their union; `reveal` is the animated slide it drives:
    //   0 = fully HIDDEN (slid entirely off-screen, nothing visible)
    //   peekReveal = the fore-edge SLIVER peeks (the awaiting-alert handle)
    //   1 = fully OUT (the whole codex on-screen, spine at the edge)
    property bool shown: false
    property bool hotEdge: false
    property bool overPanel: false
    // held true while the body scrollbar is being dragged (set by bodyRail) —
    // a drag that wanders off the board must not let the codex slide shut.
    property bool railDrag: false
    readonly property bool dockOpen: root.shown || root.hotEdge || root.overPanel
                                     || root.railDrag
    property real reveal: 0
    Behavior on reveal { NumberAnimation { duration: 200; easing.type: Easing.OutCubic } }

    // The reveal at which exactly the fore-edge sliver shows (the alert peek).
    readonly property real peekReveal: root.sliverW / root.bookW

    // ── Awaiting-alert gating ───────────────────────────────────────────────
    // The PASSIVE peek is an alert: it slides the fore-edge sliver out ONLY when
    // an agent needs a response — any session in awaiting/blocked/needs-input —
    // AND that alert hasn't yet been acknowledged. Opening the dock (any path)
    // acknowledges; the peek re-arms when `anyAwaiting` next transitions
    // false→true. The keybind toggle below is INDEPENDENT of this gating.
    property bool anyAwaiting: false
    property bool acknowledged: false

    // toggle()/show()/hide() are the UNCONDITIONAL keybind contract — summon or
    // dismiss anytime, awaiting or not.
    function show()   { root.shown = true }
    function hide()   { root.shown = false }
    function toggle() { root.shown = !root.shown }

    // The resting slide when the dock is NOT actively open: peek if an
    // unacknowledged agent-alert is live, otherwise fully hidden.
    function targetReveal() {
        if (root.dockOpen) return 1
        if (root.anyAwaiting && !root.acknowledged) return root.peekReveal
        return 0
    }
    function refreshRest() { if (!root.dockOpen) root.reveal = root.targetReveal() }

    // ── Auto-hide grace: opening slides out at once; a close (hover drop or a
    // dismiss) arms a short timer so brushing the strip doesn't flap. The timer
    // resolves to the current resting target (peek or hidden). ────────────────
    Timer { id: grace; interval: 450; onTriggered: root.reveal = root.targetReveal() }
    function evalReveal() {
        if (root.dockOpen) { grace.stop(); root.reveal = 1 }
        else grace.restart()
    }
    onDockOpenChanged: {
        if (root.dockOpen) root.acknowledged = true   // opening acknowledges the alert
        evalReveal()
    }
    onAnyAwaitingChanged: {
        if (root.anyAwaiting) root.acknowledged = false   // re-arm on a false→true event
        root.refreshRest()
    }
    onAcknowledgedChanged: root.refreshRest()
    onShownChanged: if (root.shown) book.forceActiveFocus()   // Esc when focused
    Component.onCompleted: { root.parseStage(); root.evalReveal() }

    // ── Stage watch: recompute `anyAwaiting` off sessions.json, off the CANONICAL
    // state the rewritten daemon publishes — a session needs the user exactly when
    // its `state === "awaiting"` (needsInput ⇔ awaiting). No regex derivation.
    FileView {
        id: stage
        path: "/home/khoa/Aoide/song/stage/sessions.json"
        watchChanges: true
        blockLoading: false
        printErrors: false
        onLoaded: root.parseStage()
        onFileChanged: reload()
    }
    // A tiny debounce so a heartbeat rewrite of sessions.json that does NOT change
    // the awaiting-set never re-triggers or flickers the peek: parseStage stages a
    // raw value and (re)arms this timer; the value only commits to `anyAwaiting`
    // (which drives the peek edge) once it has held steady for the interval, so a
    // transient blip between two rewrites can't re-arm the alert.
    property bool _rawAwaiting: false
    Timer {
        id: awaitDebounce
        interval: 250; repeat: false
        onTriggered: root.anyAwaiting = root._rawAwaiting
    }
    function parseStage() {
        var awaiting = false;
        try {
            var t = stage.text();
            if (t && t.trim().length > 0) {
                var o = JSON.parse(t);
                var s = (o && o.sessions) ? o.sessions : [];
                for (var i = 0; i < s.length; i++) {
                    if ((s[i].state || "").toLowerCase() === "awaiting") { awaiting = true; break; }
                }
            }
        } catch (e) { awaiting = false; }
        root._rawAwaiting = awaiting;
        if (awaiting === root.anyAwaiting) awaitDebounce.stop();  // no change → cancel any pending flip
        else awaitDebounce.restart();                            // debounce the transition
    }

    // ── Layer-shell surface (Overlay, left-edge, vertically centred) ────────
    // Anchored LEFT only → pinned left, centred vertically. Always mapped so the
    // fore-edge always peeks and the hot strip is always live; the mask (below)
    // keeps the shut dock from deadening the screen. exclusiveZone 0 reserves
    // nothing; transparent so only the codex draws.
    anchors { left: true }
    margins.left: 0
    exclusiveZone: 0
    color: "transparent"
    visible: true

    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "aoide-dock"
    // OnDemand while pinned so a click can take focus; None otherwise so a mere
    // hover-peek never holds the keyboard.
    WlrLayershell.keyboardFocus: root.shown ? WlrKeyboardFocus.OnDemand
                                            : WlrKeyboardFocus.None

    implicitWidth: root.bookW
    implicitHeight: root.panelH

    // ── Trigger: Hyprland global shortcut (aoide:dock) ─────────────────────
    GlobalShortcut {
        appid: "aoide"
        name: "dock"
        description: "Summon the Aoide center-left codex dock"
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
    readonly property real screenH: root.screen ? root.screen.height : 1080
    readonly property real maxPanelH: screenH * 0.92
    readonly property real panelH: Math.min(root.maxPanelH, 960)   // ≤ 92% of the output

    readonly property int innerW: 360        // the gadgets' native column width
    // ── ONE width source for every dock gadget ──────────────────────────────
    // All five steles in the stack take exactly this width so the column reads
    // flush — no 340-vs-360 stagger, one shared left edge. (== innerW so a gadget
    // set to gadgetW fills the flick column edge-to-edge.)
    readonly property real gadgetW: 360
    readonly property int spineW: 16         // the carved inner-edge spine
    readonly property int striationW: 26     // the outer fore-edge (page striations)
    readonly property int scrollGutter: 12   // the slim scrollbar's lane
    readonly property int bandPad: 6         // gap either side of the content column
    readonly property int coverInset: 8      // marble pad inside the keyline
    readonly property int bookW: coverInset * 2 + spineW + bandPad
                                 + innerW + scrollGutter + bandPad + striationW
    // The width of the fore-edge sliver the alert-peek shows.
    readonly property int sliverW: striationW + 8
    // A thin always-present hover strip at the very screen edge (the hot-edge),
    // live even when the dock is fully hidden.
    readonly property int hotStripW: 6

    // ══ INPUT MASK — follows the slide ════════════════════════════════════════
    // The interactive region is x:0 .. width, full height, where width tracks the
    // drawn codex (bookW·reveal) but never drops below the hot strip — so the
    // fully-hidden dock still catches an edge hover, the peek catches a click on
    // the sliver, and the open dock is fully interactive. Everything off the drawn
    // codex falls through (Region.item tracks this live geometry).
    Item {
        id: hit
        x: 0; y: 0
        height: root.panelH
        width: Math.max(root.hotStripW, root.bookW * root.reveal)
    }
    mask: Region { item: hit }

    // ── Left hot strip — always live, at the true screen edge ───────────────
    // Independent of the book's translate (a direct child of the window), so it
    // sits at screen x 0..hotStripW whether the dock is hidden, peeking, or out.
    // Hover-only — never steals a click.
    MouseArea {
        anchors.left: parent.left
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        width: root.hotStripW
        hoverEnabled: true
        acceptedButtons: Qt.NoButton
        onContainsMouseChanged: root.hotEdge = containsMouse
    }

    // ══ THE CODEX ═══════════════════════════════════════════════════════════
    Item {
        id: book
        width: root.bookW
        height: root.panelH
        focus: true

        // The slide: translate X across the full travel — fully off-screen at
        // reveal 0, spine at the edge at reveal 1, a bare sliver at peekReveal.
        transform: Translate { x: -root.bookW * (1 - root.reveal) }

        // Keyboard (best-effort — only when the layer holds focus)
        Keys.onEscapePressed: root.hide()

        // Any pointer over the drawn board keeps a hover-peek open (a passive
        // handler, so it never blocks the Flickable's wheel or the gadgets).
        HoverHandler { onHoveredChanged: root.overPanel = hovered }

        // cast shadow — lifts the codex off the marble wallpaper
        Rectangle {
            anchors.fill: cover
            anchors.leftMargin: 5; anchors.topMargin: 6
            anchors.rightMargin: -5; anchors.bottomMargin: -6
            radius: 0
            color: root.withA(root.livery.paletteFg, 0.22)
        }

        // the marble board (cover) ─────────────────────────────────────────
        Rectangle {
            id: cover
            anchors.fill: parent
            radius: 0
            color: root.livery.paletteBg
            border.color: root.livery.paletteFg
            border.width: 2

            Rectangle {                          // inset gold keyline
                anchors.fill: parent; anchors.margins: 4
                radius: 0; color: "transparent"
                border.color: root.livery.paletteAccent; border.width: 1
            }

            // the interior, inside the keyline
            Item {
                id: inner
                anchors.fill: parent
                anchors.margins: root.coverInset

                // ── SPINE: the carved inner edge (goes to the screen edge when
                // out; slides off when shut). Twin gold rules + binding stations. ─
                Rectangle {
                    id: spine
                    anchors.left: parent.left
                    anchors.top: parent.top; anchors.bottom: parent.bottom
                    width: root.spineW
                    color: root.withA(root.livery.paletteFg, 0.10)

                    Rectangle {
                        anchors.left: parent.left; anchors.leftMargin: 4
                        anchors.top: parent.top; anchors.bottom: parent.bottom
                        width: 1; color: root.withA(root.livery.paletteAccent, 0.55)
                    }
                    Rectangle {
                        anchors.right: parent.right; anchors.rightMargin: 4
                        anchors.top: parent.top; anchors.bottom: parent.bottom
                        width: 1; color: root.withA(root.livery.paletteAccent, 0.55)
                    }
                    Column {                      // stitched binding stations
                        anchors.centerIn: parent
                        spacing: 26
                        Repeater {
                            model: Math.max(4, Math.floor(root.panelH / 64))
                            Text {
                                text: "◆"
                                font.family: root.faceMusic; font.pixelSize: 7
                                color: root.withA(root.livery.paletteAccent, 0.7)
                            }
                        }
                    }
                }

                // ── FORE-EDGE: the stack of page-edge striations on the OUTER,
                // screen-facing edge — this is the sliver that peeks at rest. The
                // leaves run VERTICALLY, one thin rule per page edge down the full
                // height, over a barrel-shaded ground (dark at the board, a crest
                // highlight, dark again at the outer lip) — the block of paper seen
                // end-on, so the codex reads as SOLID and popping out of the edge.
                // An occasional gilt leaf + a ribbon bookmark. ────────────────────
                Item {
                    id: foreEdge
                    anchors.right: parent.right
                    anchors.top: parent.top; anchors.bottom: parent.bottom
                    width: root.striationW

                    Rectangle {                   // the cut-page ground, barrel-shaded
                        anchors.fill: parent
                        gradient: Gradient {
                            orientation: Gradient.Horizontal
                            GradientStop { position: 0.00; color: root.withA(root.livery.paletteFg, 0.17) }
                            GradientStop { position: 0.34; color: root.withA(root.livery.paletteFg, 0.03) }
                            GradientStop { position: 0.80; color: root.withA(root.livery.paletteFg, 0.08) }
                            GradientStop { position: 1.00; color: root.withA(root.livery.paletteFg, 0.22) }
                        }
                    }
                    Rectangle {                   // the board edge — a hard plum rule
                        anchors.left: parent.left
                        anchors.top: parent.top; anchors.bottom: parent.bottom
                        width: 1
                        color: root.withA(root.livery.paletteFg, 0.45)
                    }

                    // the leaves — one thin VERTICAL rule per ~2px across the band,
                    // heights + shading varied so the cut edge reads as real paper
                    // rather than a ruled grid.
                    Repeater {
                        model: Math.max(6, Math.floor((root.striationW - 2) / 2))
                        Rectangle {
                            x: 1 + index * 2
                            anchors.top: parent.top
                            anchors.bottom: parent.bottom
                            anchors.topMargin: (index % 5 === 0) ? 0 : 2 + ((index * 7) % 5)
                            anchors.bottomMargin: (index % 4 === 0) ? 0 : 2 + ((index * 13) % 6)
                            width: (index % 6 === 0) ? 2 : 1
                            color: (index % 7 === 3)
                                   ? root.withA(root.livery.paletteAccent, 0.30)   // a gilt leaf
                                   : root.withA(root.livery.paletteFg,
                                                0.09 + ((index * 11) % 6) * 0.018)
                        }
                    }

                    Rectangle {                   // the outer lip — the last leaf, lit
                        anchors.right: parent.right
                        anchors.top: parent.top; anchors.bottom: parent.bottom
                        width: 1
                        color: root.withA(root.livery.paletteFg, 0.38)
                    }

                    Rectangle {                   // a ribbon bookmark spilling from the top
                        anchors.right: parent.right; anchors.rightMargin: 6
                        anchors.top: parent.top
                        width: 3; height: 74
                        color: root.withA(root.livery.paletteAccent, 0.7)
                    }

                    // A click on the visible sliver opens (and acknowledges) — the
                    // third open path alongside the keybind and the hot-edge. The
                    // wheel over the fore-edge scrolls the BODY, so the peeking
                    // page-block doubles as the dock's thumb rail: hovering it
                    // already slides the codex out, and the same gesture that got
                    // you there keeps paging the stack.
                    MouseArea {
                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        onClicked: root.show()
                        onWheel: (wheel) => {
                            var d = wheel.angleDelta.y !== 0 ? wheel.angleDelta.y
                                                            : wheel.angleDelta.x
                            var span = flick.contentHeight - flick.height
                            if (span > 0) {
                                flick.cancelFlick()
                                flick.contentY = Math.max(0, Math.min(span,
                                                          flick.contentY - d * 0.6))
                            }
                            wheel.accepted = true
                        }
                    }
                }

                // ── HEADER: a clef cartouche naming the codex ────────────────
                Item {
                    id: header
                    anchors.top: parent.top
                    anchors.left: spine.right; anchors.leftMargin: root.bandPad
                    anchors.right: foreEdge.left; anchors.rightMargin: root.bandPad
                    height: 32

                    Rectangle {                   // deeper marble band
                        anchors.fill: parent; anchors.bottomMargin: 4
                        color: root.withA(root.livery.paletteFg, 0.05)
                    }
                    Row {
                        anchors.left: parent.left; anchors.leftMargin: 2
                        anchors.verticalCenter: parent.verticalCenter
                        anchors.verticalCenterOffset: -2
                        spacing: 8
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            anchors.verticalCenterOffset: -3
                            text: "𝄞"
                            font.family: root.faceMusic; font.pixelSize: 28
                            color: root.livery.paletteAccent
                        }
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            text: "AOIDE"
                            font.family: root.faceSerif; font.pixelSize: 16
                            font.weight: Font.DemiBold; font.letterSpacing: 5
                            color: root.livery.paletteFg
                        }
                    }
                    Text {                        // a small Greek touch on the right
                        anchors.right: parent.right; anchors.rightMargin: 2
                        anchors.verticalCenter: parent.verticalCenter
                        anchors.verticalCenterOffset: -2
                        text: "ᾠδή"
                        font.family: root.faceSerif; font.pixelSize: 12; font.italic: true
                        color: root.withA(root.livery.paletteAccent, 0.85)
                    }
                    Rectangle {                   // hairline gold rule under the title
                        anchors.left: parent.left; anchors.right: parent.right
                        anchors.bottom: parent.bottom
                        height: 1
                        color: root.withA(root.livery.paletteAccent, 0.5)
                    }
                }

                // ── BODY: the four gadgets, scrolling in their natural sizes ─
                Flickable {
                    id: flick
                    anchors.top: header.bottom; anchors.topMargin: 8
                    anchors.bottom: parent.bottom
                    anchors.left: spine.right; anchors.leftMargin: root.bandPad
                    anchors.right: foreEdge.left; anchors.rightMargin: root.bandPad + root.scrollGutter
                    clip: true
                    contentWidth: width
                    contentHeight: stack.height
                    boundsBehavior: Flickable.StopAtBounds
                    flickDeceleration: 3500

                    Column {
                        id: stack
                        width: flick.width
                        spacing: 16

                        // ORDER: Conductor → Usage → Terminals → Meters → Power →
                        // Herald (the notification center sits at the BOTTOM).
                        // Every gadget takes root.gadgetW so the column is flush —
                        // one shared left edge, no 340-vs-360 stagger.

                        // Conductor — the tall session roster, native height 520,
                        // wired with bridge + shared (which it declares).
                        ConductorGadget {
                            width: root.gadgetW; height: 520
                            livery: root.livery
                            bridge: root.bridge
                            shared: root.shared
                        }

                        // Usage — the claude.ai ledger stele, sits DIRECTLY under
                        // the Conductor. Content-driven height (short while the live
                        // fetch is a stub). Visible ONLY once state/usage.json exists
                        // (hasData), so a host without `aoide usage enable` shows
                        // nothing; an invisible Column child is excluded from the
                        // layout, so it collapses to zero footprint (no gap) rather
                        // than leaving a hole between Conductor and Terminals.
                        //
                        // REMOVED from the dock 2026-08-12 (khoa): the claude stele
                        // is out of the column for now. The block below is the
                        // exact re-enable (the gadget itself is untouched, and
                        // `hasData` is its own property, so nothing dangles):
                        //
                        //   UsageGadget {
                        //       id: usageGadget
                        //       width: root.gadgetW
                        //       livery: root.livery
                        //       visible: hasData
                        //   }
                        //

                        // Terminals — the tall shells roster, native height 520,
                        // wired with bridge + shared (which it declares).
                        TerminalsGadget {
                            width: root.gadgetW; height: 520
                            livery: root.livery
                            bridge: root.bridge
                            shared: root.shared
                        }

                        // Usage — the claude.ai ledger stele, sits DIRECTLY under
                        // the Terminals. Content-driven height. Visible ONLY once
                        // state/usage.json exists (hasData), so a host without
                        // `aoide usage enable` shows nothing; an invisible Column
                        // child is excluded from the layout, so it collapses to
                        // zero footprint (no gap) rather than leaving a hole.
                        UsageGadget {
                            id: usageGadget
                            width: root.gadgetW
                            livery: root.livery
                            bridge: root.bridge     // for the ❋-spark manual refresh (refreshusage verb)
                            visible: hasData
                        }

                        // Meters + Power — the compact steles, native height 268;
                        // livery ONLY (declaring an undeclared prop is an error, so no
                        // bridge/shared here). No centring wrapper anymore — they take
                        // root.gadgetW directly and share the column's left edge with
                        // the others. Their frames anchors.fill, so the wider 360
                        // width just fills.
                        MetersGadget {
                            width: root.gadgetW; height: 268
                            livery: root.livery
                        }
                        PowerVitalsGadget {
                            width: root.gadgetW; height: 268
                            livery: root.livery
                        }

                        // Herald — the notification CENTER, the dock's BOTTOM
                        // gadget (song slot; dunst owns the popups themselves
                        // since 2026-08-16). Content-height passthrough from the
                        // loaded song widget (WidgetSlot sizes to its item);
                        // width pinned here so the column stays flush.
                        WidgetSlot {
                            slot: "herald-center"
                            width: root.gadgetW
                            livery: root.livery
                            bridge: root.bridge
                            stagingEngine: root.stagingEngine
                        }

                        // Declared dock-kind widgets — the v2 expansion of the
                        // declared widget-type registry (SongGadgets.qml, the
                        // dock counterpart to SongSurfaces.qml's surface-kind
                        // host). Empty today (no committed song declares a
                        // `kind: "dock"` entry), so this contributes zero
                        // footprint and zero spacing — a structural no-op.
                        SongGadgets {
                            livery: root.livery
                            bridge: root.bridge
                            stagingEngine: root.stagingEngine
                            gadgetW: root.gadgetW
                        }
                    }
                }

                // ── SCROLLBAR: a slim gold thumb in the inner gutter, draggable
                // (ScrollRail). grabPad 4 makes the lane exactly the 12px
                // gutter — flush to the flick's edge, never over it, so a click
                // meant for a gadget's right edge still reaches the gadget. ───
                ScrollRail {
                    id: bodyRail
                    flick: flick
                    railW: 4
                    minThumb: 28
                    grabPad: 4
                    trackColor: root.withA(root.livery.paletteFg, 0.12)
                    thumbColor: root.withA(root.livery.paletteAccent, 0.8)
                    anchors.top: flick.top; anchors.bottom: flick.bottom
                    anchors.left: flick.right; anchors.leftMargin: 4
                    // a drag holds the codex out even if the pointer wanders off
                    // the board (the HoverHandler drops, the grab does not)
                    onDraggingChanged: root.railDrag = dragging
                }

                // ── "more below" hint — fades when the stack bottoms out ─────
                Text {
                    id: moreHint
                    anchors.horizontalCenter: flick.horizontalCenter
                    anchors.bottom: flick.bottom; anchors.bottomMargin: 2
                    text: "▽ more"
                    font.family: root.faceMono; font.pixelSize: 11
                    color: root.withA(root.livery.paletteAccent, 0.9)
                    visible: flick.visibleArea.heightRatio < 0.999
                    opacity: (flick.visibleArea.yPosition
                              + flick.visibleArea.heightRatio) < 0.995 ? 1 : 0
                    Behavior on opacity { NumberAnimation { duration: 180 } }
                    SequentialAnimation on anchors.bottomMargin {
                        running: moreHint.visible && moreHint.opacity > 0
                        loops: Animation.Infinite; alwaysRunToEnd: true
                        NumberAnimation { to: 5; duration: 620; easing.type: Easing.InOutSine }
                        NumberAnimation { to: 2; duration: 620; easing.type: Easing.InOutSine }
                    }
                }
            }
        }
    }
}
