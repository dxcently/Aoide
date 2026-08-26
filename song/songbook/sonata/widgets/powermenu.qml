// powermenu.qml — sonata's "powermenu" slot (was AoideExodos.qml, moved out
// of the facet as of the widget-slot expansion, CONTRACTS.md §5). Hosted by
// `SurfaceSlot`, not `WidgetSlot` — this root is a `PanelWindow`, not an
// `Item`, so it owns its own layer, namespace, keyboard focus, and
// GlobalShortcut; that contract travels with the slot (see
// modules/facets/quickshell/qml/slots.md). Moved as one inseparable unit —
// the deal animation, the layer-shell setup, the GlobalShortcut all still
// belong together in this one file.
//
// ΕΞΟΔΟΣ — in Greek tragedy the exodos is the closing song, the ode the chorus
// sings as it leaves the stage. That is exactly what a powermenu is in this
// house: the ways the performance ends. Six endings, each a REAL notation mark
// for how a piece of music stops:
//
//   LOCK      𝄐  fermata      — the note is HELD until the player returns
//   LOGOUT    𝄌  coda         — jump to the coda; the chorus files out,
//                               the hall stays open
//   SUSPEND   𝄾  grand pause  — a breath of total silence, then the piece
//                               resumes exactly where it stopped
//   HIBERNATE 𝄻  tacet        — silent for the whole movement; the score is
//                               closed and shelved, to reopen on the same bar
//   REBOOT    𝄇  da capo      — repeat sign: play it again from the top
//   SHUTDOWN  𝄂  fine         — the final barline; the piece ends
//
// Each ending is its own STELE — the AoideBar workspace-note precedent (one
// glyph per id, same family, individually legible) raised to full panes: every
// stele carries its OWN glyph, its OWN signature hue, its OWN frieze band, and
// its OWN Greek whisper, while all six share one temple-bay chrome (glass body,
// 2px ink border, inset signature keyline, ┤ term ├ TUI frame top and a 𝄂
// closing frame bottom, carved-serif name, radius 0). Severity reads left to
// right: gold → verdigris → murex → aegean → clay → terracotta.
//
// Colour discipline: every colour is a `livery` role read — zero hex. The one
// laurel (`paletteHot`) is the hovered/selected stele's keyline + ♪ mark,
// nothing else. wireCyan appears once as LOGOUT's SIGNATURE hue (the grammar's
// Meters precedent: the ≤0.5-alpha cap binds the STRUCTURAL role, not the
// hue — structural verdigris here, the title rules, stays ≤0.35).
//
// ── The glass (the Grimoire recipe, verbatim) ──────────────────────────────
// Full-screen transparent Overlay layer surface, namespace "aoide-powermenu".
// The compositor facet (modules/facets/compositor/default.nix) gives that
// namespace `layerrule blur` + `ignore_alpha 0.05` + hyprglass Liquid Glass,
// so the desktop behind is visible, blurred, and gently refracted through the
// ink scrim and the translucent steles — the frosted-depth read.
//
// ── Wiring ─────────────────────────────────────────────────────────────────
// shell.qml instantiates a `SurfaceSlot { id: powermenuSlot; slot: "powermenu" }`,
// which resolves + dynamically creates this file and hands `powermenuSlot.item`
// to AoideBar (the loaded song widget itself, not a declared-by-name
// instance — SurfaceSlot's whole point); the bar's treble clef
// (𝄞, "the powermenu key") calls `.toggle()` on that handle directly. A
// Hyprland global shortcut (`aoide:powermenu`) is the second trigger — the
// launcher's in-process inbound idiom. A click on a stele sends `{ cmd: "power", action: "…" }` through
// ShellBridge's socket — the Rust shellbridge daemon
// (pkgs/aoide/crates/conduct/src/shellbridge.rs) parses it and spawns the
// system action (hyprlock / hyprctl dispatch exit / systemctl …). QML never
// shells out; the bridge is the gate.
//
// Escape or a scrim click dismisses. ←/→ (or h/l) walk the steles, Enter
// commits the selected ending.
//
// ── Motion ─────────────────────────────────────────────────────────────────
// The summon DEALS the hand: a linear master clock (`rise`) that each stele
// windows per-index (`dealT`), so the six cards slide in staggered, each
// unswinging a true Y-axis Rotation from 72° to flat — the Grimoire cover-
// fold's real perspective foreshortening (AoideLauncher.qml), per card
// instead of per book. Hover pops the card: OutBack scale + a small lift,
// with the laurel keyline/glyph/tick cross-fading in (nothing snaps).

import QtQuick
import Quickshell
import Quickshell.Wayland
import Quickshell.Hyprland

PanelWindow {
    id: root

    // ── Note + bridge dependencies (injected by shell.qml) ─────────────────
    required property var livery
    required property var bridge

    // ── Type voices (the Conductor family) ─────────────────────────────────
    readonly property string faceSerif: "Noto Serif"                // carved marble
    readonly property string faceMono:  "JetBrainsMono Nerd Font"   // the terminal
    readonly property string faceMusic: "Noto Music"                // notation

    // alpha on a role string
    function withA(cstr, a) {
        var c = Qt.color(cstr)
        return Qt.rgba(c.r, c.g, c.b, a)
    }

    // ── Visibility gate + the deal ─────────────────────────────────────────
    // `rise` is a LINEAR master clock (0 shut .. 1 open); each stele carves
    // its own staggered window out of it (`dealT` below) and applies its own
    // OutCubic ease inside that window — so the six cards deal onto the table
    // one after another, not in lockstep. Each card unswings a REAL Y-axis
    // Rotation (the Grimoire cover-fold mechanism, AoideLauncher.qml — true
    // perspective foreshortening, never a 2D skew) as it slides into its slot.
    // Hiding runs the clock back fast: the hand is gathered up, last card
    // first.
    property bool shown: false
    property real rise: 0
    Behavior on rise {
        NumberAnimation {
            duration: root.shown ? 560 : 200
            easing.type: Easing.Linear
        }
    }

    function show()   { root.shown = true }
    function hide()   { root.shown = false }
    function toggle() { root.shown = !root.shown }

    onShownChanged: {
        root.rise = root.shown ? 1 : 0
        if (root.shown) {
            root.sel = -1
            keyCatcher.forceActiveFocus()
        }
    }

    // Dispatch an ending through the bridge (the only outbound path from QML)
    // and dismiss. The daemon owns the actual spawn.
    function invoke(action) {
        root.bridge.sendCommand({ cmd: "power", action: action })
        root.hide()
    }

    // ── Layer-shell surface (Overlay, full-screen, transparent) — the
    // Grimoire's exact shape; the namespace is the glass contract. ──────────
    anchors { top: true; bottom: true; left: true; right: true }
    exclusiveZone: 0
    color: "transparent"
    visible: root.shown || root.rise > 0.01
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "aoide-powermenu"
    WlrLayershell.keyboardFocus: root.shown ? WlrKeyboardFocus.Exclusive
                                            : WlrKeyboardFocus.None

    // ── Second trigger: Hyprland global shortcut (aoide:powermenu) ─────────
    // The launcher's in-process inbound idiom (see AoideLauncher.qml's
    // rationale — no new socket, no aoided command). The bar's clef stays the
    // primary key; this gives the compositor a bindable hook and makes the
    // LIVE instance summonable programmatically
    // (`hyprctl dispatch global aoide:powermenu`).
    GlobalShortcut {
        appid: "aoide"
        name: "powermenu"
        description: "Summon the Exodos powermenu"
        onPressed: root.toggle()
    }

    // ── Selection (hover or keys). -1 = nothing chosen; the chosen stele is
    // the surface's ONE laurel element. ─────────────────────────────────────
    property int sel: -1

    // ── The six endings — glyph, hue, frieze, whisper all per-stele ────────
    readonly property var endings: [
        { action: "lock",      name: "LOCK",      term: "fermata",
          greek: "κλείς",           glyph: "𝄐", gs: 56,
          frieze: "◆───◆───◆",      hue: root.livery.paletteAccent },
        { action: "logout",    name: "LOGOUT",    term: "coda",
          greek: "ἔξοδος χοροῦ",    glyph: "𝄌", gs: 56,
          frieze: "⌐¬ ⌐¬ ⌐¬ ⌐¬",    hue: root.livery.wireCyan },
        { action: "suspend",   name: "SUSPEND",   term: "grand pause",
          greek: "ἀνάπαυσις",       glyph: "𝄾", gs: 56,
          frieze: "╌╌ ╌╌ ╌╌ ╌╌",    hue: root.livery.violet },
        { action: "hibernate", name: "HIBERNATE", term: "tacet",
          greek: "χειμερία νάρκη",  glyph: "𝄻", gs: 64,
          frieze: "▔▁▔▁▔▁▔",        hue: root.livery.holoBlue },
        { action: "reboot",    name: "REBOOT",    term: "da capo",
          greek: "ἀπ᾿ ἀρχῆς",       glyph: "𝄇", gs: 54,
          frieze: "┏┛┗┓┏┛┗┓",       hue: root.livery.base09 },
        { action: "shutdown",  name: "SHUTDOWN",  term: "fine",
          greek: "τέλος",           glyph: "𝄂", gs: 54,
          frieze: "══════════",     hue: root.livery.paletteUrgent }
    ]

    // ══ Dim scrim — the ink veil the glass frosts through; a click anywhere
    // outside the steles stays. Keyed off the song's ink (launcher idiom). ═══
    MouseArea {
        anchors.fill: parent
        onClicked: root.hide()
        Rectangle {
            anchors.fill: parent
            color: root.withA(root.livery.paletteFg, 0.30)
            opacity: root.rise
        }
    }

    // ── Keyboard: walk the row, commit, or stay ────────────────────────────
    Item {
        id: keyCatcher
        focus: true
        Keys.onPressed: function (event) {
            var n = root.endings.length
            if (event.key === Qt.Key_Escape) {
                root.hide(); event.accepted = true
            } else if (event.key === Qt.Key_Left || event.key === Qt.Key_H) {
                root.sel = (root.sel <= 0) ? n - 1 : root.sel - 1
                event.accepted = true
            } else if (event.key === Qt.Key_Right || event.key === Qt.Key_L
                       || event.key === Qt.Key_Tab) {
                root.sel = (root.sel < 0 || root.sel >= n - 1) ? 0 : root.sel + 1
                event.accepted = true
            } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                if (root.sel >= 0 && root.sel < n)
                    root.invoke(root.endings[root.sel].action)
                event.accepted = true
            }
        }
    }

    // ── One ending — a glass stele in the shared temple-bay chrome ─────────
    component EndingStele: Rectangle {
        id: stele
        required property var modelData
        required property int index
        readonly property bool lit: root.sel === stele.index
        readonly property color hue: stele.modelData.hue

        width: 196
        height: 306
        radius: 0
        color: root.withA(root.livery.paletteBg, stele.lit ? 0.85 : 0.74)
        Behavior on color { ColorAnimation { duration: 160 } }
        border.color: root.livery.paletteFg
        border.width: 2

        // ── the deal — this card's slice of the master clock ───────────────
        // Window i opens at i·stag on the linear `rise` and spans the rest;
        // OutCubic applied INSIDE the window so every card lands softly no
        // matter where its window sits on the clock.
        readonly property real dealT: {
            var stag = 0.085
            var span = 1 - stag * (root.endings.length - 1)
            var p = (root.rise - stele.index * stag) / span
            if (p < 0) p = 0
            if (p > 1) p = 1
            return 1 - Math.pow(1 - p, 3)
        }
        opacity: Math.min(1, stele.dealT * 1.6)

        // hover pop — the card lifts toward the cursor; OutBack overshoot on
        // the scale so it reads as a pop, not a resize
        scale: stele.lit ? 1.045 : 1.0
        Behavior on scale { NumberAnimation { duration: 170; easing.type: Easing.OutBack } }
        property real lift: stele.lit ? -7 : 0
        Behavior on lift { NumberAnimation { duration: 170; easing.type: Easing.OutCubic } }

        // The card in flight: swings flat about its own vertical axis (true
        // 3D perspective — the rectangle visibly foreshortens mid-deal) while
        // sliding up from the dealer's side into its slot. Transforms leave
        // the Row's layout untouched, so seated geometry is exact.
        transform: [
            Rotation {
                origin.x: stele.width / 2
                origin.y: stele.height / 2
                axis { x: 0; y: 1; z: 0 }
                angle: (1 - stele.dealT) * 72
            },
            Translate {
                x: -120 * (1 - stele.dealT)
                y: 34 * (1 - stele.dealT) + stele.lift
            }
        ]

        Rectangle {   // gloss sheen (the Aero read over the compositor glass)
            anchors.fill: parent; anchors.margins: 2
            gradient: Gradient {
                GradientStop { position: 0.0;  color: Qt.rgba(1, 1, 1, 0.13) }
                GradientStop { position: 0.45; color: Qt.rgba(1, 1, 1, 0.03) }
                GradientStop { position: 0.5;  color: Qt.rgba(1, 1, 1, 0.00) }
                GradientStop { position: 1.0;  color: Qt.rgba(1, 1, 1, 0.03) }
            }
        }
        Rectangle {   // inset keyline — signature hue; the laurel when chosen
            anchors.fill: parent; anchors.margins: 5
            color: "transparent"
            border.color: stele.lit ? root.livery.paletteHot
                                    : root.withA(stele.hue, 0.7)
            border.width: stele.lit ? 2 : 1
            Behavior on border.color { ColorAnimation { duration: 160 } }
            Behavior on border.width { NumberAnimation { duration: 160 } }
        }

        // ── top TUI frame: ┌── ┤ term ├ ──┐ (the score direction) ──────────
        Item {
            id: topFrame
            anchors.top: parent.top; anchors.topMargin: 14
            anchors.left: parent.left; anchors.leftMargin: 12
            anchors.right: parent.right; anchors.rightMargin: 12
            height: 14
            Text {
                id: cornerTL
                anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter
                text: "┌"; font.family: root.faceMono; font.pixelSize: 11
                color: root.withA(root.livery.paletteFg, 0.6)
            }
            Text {
                id: cornerTR
                anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                text: "┐"; font.family: root.faceMono; font.pixelSize: 11
                color: root.withA(root.livery.paletteFg, 0.6)
            }
            Text {
                id: termLabel
                anchors.centerIn: parent
                text: "┤ " + stele.modelData.term + " ├"
                font.family: root.faceMono; font.pixelSize: 10
                color: root.withA(root.livery.paletteFg, 0.8)
            }
            Rectangle {
                anchors.verticalCenter: parent.verticalCenter
                anchors.left: cornerTL.right; anchors.right: termLabel.left
                anchors.leftMargin: 1; anchors.rightMargin: 2
                height: 1; color: root.withA(root.livery.paletteFg, 0.35)
            }
            Rectangle {
                anchors.verticalCenter: parent.verticalCenter
                anchors.left: termLabel.right; anchors.right: cornerTR.left
                anchors.leftMargin: 2; anchors.rightMargin: 1
                height: 1; color: root.withA(root.livery.paletteFg, 0.35)
            }
        }

        // ── the mark — this ending's notation glyph, signature hue ─────────
        // Hover FLOURISH: on top of the shared family signature (pop, lift,
        // laurel) each mark answers the cursor in its OWN character — the
        // fermata breathes like a held note, the coda rocks on its axis, the
        // grand pause dies away (morendo), the tacet rest settles deeper,
        // da capo wheels its repeat sign a full turn, fine cinches its
        // barlines tight. One flourish per card, looping while lit, every
        // animated value reset clean when the cursor leaves. The sink and
        // cinch ride transforms, not anchors, so the text stack below never
        // moves.
        Text {
            id: glyphMark
            anchors.horizontalCenter: parent.horizontalCenter
            anchors.top: parent.top; anchors.topMargin: 44
            height: 104
            verticalAlignment: Text.AlignVCenter
            text: stele.modelData.glyph
            font.family: root.faceMusic
            font.pixelSize: stele.modelData.gs
            color: root.withA(stele.hue, stele.lit ? 1.0 : 0.88)
            Behavior on color { ColorAnimation { duration: 160 } }

            property real sinkOff: 0
            transform: [
                Scale {
                    id: cinch
                    origin.x: glyphMark.width / 2
                    origin.y: glyphMark.height / 2
                    xScale: 1
                },
                Translate { y: glyphMark.sinkOff }
            ]

            SequentialAnimation {   // LOCK 𝄐 — the held note breathes
                running: stele.lit && stele.modelData.action === "lock"
                loops: Animation.Infinite
                onStopped: glyphMark.scale = 1
                NumberAnimation { target: glyphMark; property: "scale"; to: 1.09; duration: 700; easing.type: Easing.InOutSine }
                NumberAnimation { target: glyphMark; property: "scale"; to: 1.0;  duration: 700; easing.type: Easing.InOutSine }
            }
            SequentialAnimation {   // LOGOUT 𝄌 — the coda rocks on its axis
                running: stele.lit && stele.modelData.action === "logout"
                loops: Animation.Infinite
                onStopped: glyphMark.rotation = 0
                NumberAnimation { target: glyphMark; property: "rotation"; to: 10;  duration: 450; easing.type: Easing.InOutSine }
                NumberAnimation { target: glyphMark; property: "rotation"; to: -10; duration: 900; easing.type: Easing.InOutSine }
                NumberAnimation { target: glyphMark; property: "rotation"; to: 0;   duration: 450; easing.type: Easing.InOutSine }
            }
            SequentialAnimation {   // SUSPEND 𝄾 — morendo, dying toward quiet
                running: stele.lit && stele.modelData.action === "suspend"
                loops: Animation.Infinite
                onStopped: glyphMark.opacity = 1
                NumberAnimation { target: glyphMark; property: "opacity"; to: 0.3; duration: 900; easing.type: Easing.InOutSine }
                NumberAnimation { target: glyphMark; property: "opacity"; to: 1.0; duration: 900; easing.type: Easing.InOutSine }
            }
            SequentialAnimation {   // HIBERNATE 𝄻 — the rest settles deeper
                running: stele.lit && stele.modelData.action === "hibernate"
                loops: Animation.Infinite
                onStopped: glyphMark.sinkOff = 0
                NumberAnimation { target: glyphMark; property: "sinkOff"; to: 6; duration: 1000; easing.type: Easing.InOutSine }
                NumberAnimation { target: glyphMark; property: "sinkOff"; to: 0; duration: 1000; easing.type: Easing.InOutSine }
            }
            NumberAnimation {       // REBOOT 𝄇 — da capo, the sign wheels round
                target: glyphMark; property: "rotation"
                running: stele.lit && stele.modelData.action === "reboot"
                loops: Animation.Infinite
                from: 0; to: 360; duration: 1500
                onStopped: glyphMark.rotation = 0
            }
            SequentialAnimation {   // SHUTDOWN 𝄂 — fine cinches the bars tight
                running: stele.lit && stele.modelData.action === "shutdown"
                loops: Animation.Infinite
                onStopped: cinch.xScale = 1
                NumberAnimation { target: cinch; property: "xScale"; to: 0.72; duration: 550; easing.type: Easing.InOutSine }
                NumberAnimation { target: cinch; property: "xScale"; to: 1.0;  duration: 550; easing.type: Easing.InOutSine }
            }
        }

        // ── the carved name + the ♪ laurel tick when chosen ────────────────
        // The tick sits OUTSIDE the centring (anchored beside the name) and
        // fades rather than toggling visible, so the name never jumps when
        // the laurel arrives.
        Text {
            id: nameRow
            anchors.horizontalCenter: parent.horizontalCenter
            anchors.top: glyphMark.bottom; anchors.topMargin: 14
            text: stele.modelData.name
            font.family: root.faceSerif
            font.pixelSize: 15
            font.letterSpacing: 4
            font.weight: Font.DemiBold
            color: root.livery.paletteFg
        }
        Text {
            anchors.right: nameRow.left; anchors.rightMargin: 7
            anchors.verticalCenter: nameRow.verticalCenter
            text: "♪"
            font.family: root.faceMusic; font.pixelSize: 14
            color: root.livery.paletteHot
            opacity: stele.lit ? 1 : 0
            Behavior on opacity { NumberAnimation { duration: 160 } }
        }

        // ── the Greek whisper ──────────────────────────────────────────────
        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            anchors.top: nameRow.bottom; anchors.topMargin: 7
            text: stele.modelData.greek
            font.family: root.faceSerif
            font.pixelSize: 11
            font.italic: true
            color: root.withA(stele.hue, 0.9)
        }

        // ── the frieze — this stele's own ornament course ───────────────────
        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            anchors.bottom: bottomFrame.top; anchors.bottomMargin: 12
            text: stele.modelData.frieze
            font.family: root.faceMono
            font.pixelSize: 12
            color: root.withA(stele.hue, 0.8)
        }

        // ── bottom frame closing on the final barline: └── ┤ 𝄂 ├ ──┘ ───────
        Item {
            id: bottomFrame
            anchors.bottom: parent.bottom; anchors.bottomMargin: 14
            anchors.left: parent.left; anchors.leftMargin: 12
            anchors.right: parent.right; anchors.rightMargin: 12
            height: 14
            Text {
                id: cornerBL
                anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter
                text: "└"; font.family: root.faceMono; font.pixelSize: 11
                color: root.withA(root.livery.paletteFg, 0.6)
            }
            Text {
                id: cornerBR
                anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                text: "┘"; font.family: root.faceMono; font.pixelSize: 11
                color: root.withA(root.livery.paletteFg, 0.6)
            }
            Text {
                id: finisMark
                anchors.centerIn: parent
                text: "┤ 𝄂 ├"
                font.family: root.faceMono; font.pixelSize: 10
                color: root.withA(root.livery.paletteFg, 0.65)
            }
            Rectangle {
                anchors.verticalCenter: parent.verticalCenter
                anchors.left: cornerBL.right; anchors.right: finisMark.left
                anchors.leftMargin: 1; anchors.rightMargin: 2
                height: 1; color: root.withA(root.livery.paletteFg, 0.35)
            }
            Rectangle {
                anchors.verticalCenter: parent.verticalCenter
                anchors.left: finisMark.right; anchors.right: cornerBR.left
                anchors.leftMargin: 2; anchors.rightMargin: 1
                height: 1; color: root.withA(root.livery.paletteFg, 0.35)
            }
        }

        MouseArea {
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onEntered: root.sel = stele.index
            onExited: if (root.sel === stele.index) root.sel = -1
            onClicked: root.invoke(stele.modelData.action)
        }
    }

    // ══ THE ASSEMBLY — inscription over the row of steles ══════════════════
    Item {
        id: rig
        anchors.centerIn: parent
        anchors.verticalCenterOffset: -14 + 18 * (1 - root.rise)
        width: steleRow.width
        height: 96 + steleRow.height + 46
        opacity: root.rise

        MouseArea { anchors.fill: parent; onClicked: {} }   // swallow clicks

        // ── the inscription — 𝄂 ΕΞΟΔΟΣ 𝄂 over a hairline rule ──────────────
        Item {
            id: heading
            anchors.top: parent.top
            anchors.horizontalCenter: parent.horizontalCenter
            width: parent.width
            height: 72

            Row {
                id: titleRow
                anchors.horizontalCenter: parent.horizontalCenter
                anchors.top: parent.top
                spacing: 18
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: "𝄂"
                    font.family: root.faceMusic; font.pixelSize: 24
                    color: root.withA(root.livery.paletteAccent, 0.95)
                }
                Text {
                    text: "ΕΞΟΔΟΣ"
                    font.family: root.faceSerif
                    font.pixelSize: 27
                    font.letterSpacing: 12
                    font.weight: Font.DemiBold
                    color: root.livery.paletteBg
                    style: Text.Normal
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: "𝄂"
                    font.family: root.faceMusic; font.pixelSize: 24
                    color: root.withA(root.livery.paletteAccent, 0.95)
                }
            }

            // structural rule — verdigris, capped well under 0.5
            Rectangle {
                anchors.top: titleRow.bottom; anchors.topMargin: 9
                anchors.horizontalCenter: parent.horizontalCenter
                width: parent.width * 0.62
                height: 1
                color: root.withA(root.livery.wireCyan, 0.35)
            }

            Text {
                anchors.top: titleRow.bottom; anchors.topMargin: 17
                anchors.horizontalCenter: parent.horizontalCenter
                text: "the closing song — πῶς τελευτᾷ ἡ ᾠδή;"
                font.family: root.faceMono
                font.pixelSize: 11
                color: root.withA(root.livery.paletteBg, 0.75)
            }
        }

        // ── the six steles ─────────────────────────────────────────────────
        Row {
            id: steleRow
            anchors.top: heading.bottom; anchors.topMargin: 24
            anchors.horizontalCenter: parent.horizontalCenter
            spacing: 22
            Repeater {
                model: root.endings
                EndingStele { }
            }
        }

        // ── the stay line ──────────────────────────────────────────────────
        Text {
            anchors.top: steleRow.bottom; anchors.topMargin: 22
            anchors.horizontalCenter: parent.horizontalCenter
            text: "← → choose · ⏎ so be it · esc μένε — stay ( ˘ω˘ )ﾉ"
            font.family: root.faceMono
            font.pixelSize: 11
            color: root.withA(root.livery.paletteBg, 0.7)
        }
    }
}
