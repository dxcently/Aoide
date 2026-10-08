// ricemode.qml — sonata's "ricemode" slot: the ONE rice-mode control. Every
// song's bar embeds it as `WidgetSlot { slot: "ricemode" }`, and this body is
// the floor for every song that does not dress the slot itself.
//
// WidgetSlot-hosted Item with no extras beyond the universal `livery`/`bridge`
// (pkgs/lyra-shell/qml/slots.md). It reports its own footprint and reaches the
// engine only through `bridge`: it starts no process and imports no compositor
// module.
//
// ── The face ────────────────────────────────────────────────────────────────
// Glyph + word + colour per mode (the vocabulary comment below), read live off
// livery.riceMode (stage/mode.json). A DRAWN chevron follows the word and marks
// the second gesture, so the picker is on screen at rest and not an invisible
// click that rots (bar.qml's header: a second invisible path to a command is
// not a shortcut). It is drawn, two 1px rotated rules, because hazards.md §1
// holds that any non-ASCII glyph is unproven on this font stack until seen. A
// bridge without `riceDrafts` has no picker, and the chevron goes with it.
//
// ── The gestures (khoa, 2026-10-07) ─────────────────────────────────────────
//   left click             bridge.toggleRiceMode()   staging ⇄ declarative
//   right OR middle click  the draft picker: the staged song's saved drafts,
//                          `+ new draft`, and `save current as draft`, which is
//                          offered in staging only (in draft mode writes already
//                          land in the draft, in declarative nothing is unsaved)
// A toggle, an enter and a new each change the mode, so the word reads `…` and
// every click is swallowed until livery.riceMode or the draft name changes, or
// 10s pass: the double click that sent two toggles. A `save` changes no mode and
// holds nothing.
//
// ── The picker ──────────────────────────────────────────────────────────────
// A PopupWindow hung from the bar's bottom edge (the anchor rect runs from the
// cell down to the window's end), created on open and
// destroyed on close so no hover state carries from one open to the next. It
// closes on a row action, on any click on the cell, and 600ms after the pointer
// is over neither the cell nor the popup. There is no text input, so no
// keyboard focus and no focus grab (hazards.md §4): the CLI mints a new draft's
// name. Quickshell 0.3.1's PopupAnchor.adjustment defaults to SlideX|SlideY
// (read off a throwaway PopupWindow on this rig), which has no flip, so FlipY
// is added and a bottom-edge bar opens it upward. Every string that came over
// the bridge is painted Text.PlainText.

import QtQuick
import Quickshell

Item {
    id: root

    required property var livery
    required property var bridge

    implicitWidth: face.implicitWidth
    implicitHeight: face.implicitHeight

    function withA(cstr, a) {
        var c = Qt.color(cstr)
        return Qt.rgba(c.r, c.g, c.b, a)
    }

    // ── Rice mode vocabulary (Aoide-native — livery.riceMode) ───────────────
    // The rice engine's edit-state, one of exactly three strings (storage::
    // mode's RiceMode, lowercase on the wire, hot from stage/mode.json via
    // LiveryState): is this manuscript under the pen right now?
    //   declarative → || decl — the measure is CLOSED. Live writes refused;
    //     what's true is what the last home-manager switch baked. Plain
    //     ASCII double bar reads as "finished and published" without
    //     touching the font's Unicode fallback chain at all. Resting ink,
    //     dimmed — the safe default should recede, not glow. (NOT 𝄽: the
    //     idle rest already works two jobs on this strip — mute + net-down —
    //     a third would let "𝄽 decl" sit two cells from "𝄽 off". TWO glyphs
    //     were tried and rejected here after live testing this session: 𝄂
    //     U+1D102 renders as a bare "|" fallback, and ‖ U+2016 — despite
    //     being common General Punctuation, not a rare SMP symbol — STILL
    //     rendered wrong (a stray "/") on this font stack. Lesson sharpened
    //     past the trayToggle scar: it's not just rare SMP glyphs that need
    //     live verification before trusting them, ANY non-ASCII glyph does,
    //     on this stack. Plain "||" sidesteps the whole fallback chain.)
    //   staging     → ♪ stage — the measure is OPEN, a note under the pen:
    //     hot-load unlocked, rice stage/cover set write the live desktop.
    //     Gold — the bar's open/active register (open toggles, the active
    //     workspace) AND the state tier's own working→gold partnering
    //     (grammar §2): the engine is literally in its working state.
    //   draft       → 𝄋 draft — dal segno: stage writes route through a
    //     saved mark (a draft snapshot), never the committed song. Aegean
    //     holoBlue — the PREVIEW register (the workspace hover-ring's
    //     "a copy, not the real thing"), distinct from both resting ink
    //     and live gold. Not urgent: no mode is an alarm.
    // Unknown strings fall through to the declarative row — the same
    // safe-default reading LiveryState and mode.rs apply to an absent or
    // corrupt marker.
    function modeGlyph(m) {
        if (m === "staging") return "♪"
        if (m === "draft")   return "𝄋"
        return "||"
    }
    function modeWord(m) {
        if (m === "staging") return "stage"
        if (m === "draft")   return "draft"
        return "decl"
    }
    function modeColor(m) {
        if (m === "staging") return root.livery.paletteAccent
        if (m === "draft")   return root.livery.holoBlue
        return root.livery.paletteFg
    }

    // ── State ───────────────────────────────────────────────────────────────
    readonly property string mode: root.livery.riceMode
    readonly property string draft: (root.livery.modeRaw && root.livery.modeRaw.draft) || ""
    readonly property bool canPick: !!root.bridge && typeof root.bridge.riceDrafts === "function"

    property bool pending: false       // a mode change was sent and has not landed
    property bool open: false          // the picker
    property var answer: null          // the last riceDrafts reply, null while asking
    property int asked: 0              // which ask a reply belongs to
    property Item hotRow: null         // the picker row under the pointer
    readonly property var drafts: root.answer && root.answer.ok && Array.isArray(root.answer.drafts)
                                  ? root.answer.drafts : []

    onModeChanged: root.pending = false
    onDraftChanged: root.pending = false
    Timer { interval: 10000; running: root.pending; onTriggered: root.pending = false }

    function gesture(button) {
        var wasOpen = root.open
        root.open = false
        if (root.pending) return
        if (button === Qt.LeftButton) {
            root.pending = true
            root.bridge.toggleRiceMode()
        } else if (root.canPick && !wasOpen) {
            root.answer = null
            var ask = ++root.asked
            root.open = true
            root.bridge.riceDrafts(function (reply) { if (ask === root.asked) root.answer = reply })
        }
    }

    // `save` never changes the mode; `enter` and `new` do.
    function act(action, name) {
        root.open = false
        if (action !== "save") root.pending = true
        root.bridge.riceDraft(action, name)
    }

    // ── The face ────────────────────────────────────────────────────────────
    Row {
        id: face
        spacing: 8
        // Declarative — locked, at rest, the ~always state — recedes like the
        // net cell's dead-link register; both unlocked modes read at full
        // strength (they're the news). A swallowed click reads the same dim.
        opacity: root.pending || root.mode === "declarative" ? 0.55 : 1.0

        // NO font.bold — the bold monospace face silently drops rare SMP
        // glyphs (the fermata scar on bar.qml's trayToggle, hazards.md §1),
        // and draft's 𝄋 is one; regular weight is the verified-safe rendering.
        Text {
            id: word
            anchors.verticalCenter: parent.verticalCenter
            // `…` pads to the word it replaces so the cell never changes width.
            text: root.modeGlyph(root.mode) + " " + (root.pending
                  ? "…" + " ".repeat(root.modeWord(root.mode).length - 1)
                  : root.modeWord(root.mode))
            color: root.modeColor(root.mode)
            font.family: "monospace"
            font.pixelSize: 14
        }

        // The second gesture, drawn: a 7×4 chevron, two 1px rules meeting at
        // the bottom, 8px after the word (the Row's spacing, as cadenza's gap),
        // centred on the word's x-height: the baseline less half a 6px x-height
        // and the chevron's own 2.
        Item {
            y: word.y + word.baselineOffset - 5
            visible: root.canPick
            width: 7
            height: 4
            Repeater {
                model: [40.6, -40.6]
                Rectangle {
                    required property real modelData
                    width: 5
                    height: 1
                    x: modelData > 0 ? -0.75 : 2.75
                    y: 1.5
                    rotation: modelData
                    antialiasing: true
                    color: root.modeColor(root.mode)
                }
            }
        }
    }

    HoverHandler { id: cellHover }
    MouseArea {
        anchors.fill: parent
        acceptedButtons: Qt.LeftButton | Qt.RightButton | Qt.MiddleButton
        cursorShape: Qt.PointingHandCursor
        onClicked: function (mouse) { root.gesture(mouse.button) }
    }

    // ── The picker ──────────────────────────────────────────────────────────
    // One row of the plate: a label, an optional drawn dot, hover band, tap.
    component PickRow: Item {
        id: row
        property string label: ""
        property real ink: 0.88
        property bool marked: false       // the dot: the draft the stage is routed into
        property bool live: true          // false for a header or a message
        readonly property bool hot: root.hotRow === row
        signal activated()

        width: parent ? parent.width : 0
        height: 20

        Rectangle {
            anchors.fill: parent
            visible: row.live && row.hot
            color: root.livery.barFg
            opacity: 0.16
        }
        Rectangle {
            visible: row.marked
            x: 7
            anchors.verticalCenter: parent.verticalCenter
            width: 6
            height: 6
            radius: 3
            color: root.modeColor("draft")
        }
        Text {
            x: 20
            width: parent.width - 26
            anchors.verticalCenter: parent.verticalCenter
            text: row.label
            textFormat: Text.PlainText
            elide: Text.ElideRight
            color: root.livery.barFg
            opacity: row.ink
            font.family: "monospace"
            font.pixelSize: 12
        }
        HoverHandler {
            enabled: row.live
            onHoveredChanged: {
                if (hovered) root.hotRow = row
                else if (root.hotRow === row) root.hotRow = null
            }
        }
        TapHandler {
            enabled: row.live
            onTapped: row.activated()
        }
    }

    LazyLoader {
        active: root.open

        PopupWindow {
            visible: true
            color: "transparent"
            anchor.item: root
            anchor.rect.x: 0
            anchor.rect.y: 0
            anchor.rect.width: root.width
            anchor.rect.height: root.QsWindow.window.height - root.mapToItem(null, 0, 0).y
            anchor.edges: Edges.Bottom
            anchor.gravity: Edges.Bottom
            anchor.adjustment: PopupAdjustment.FlipY | PopupAdjustment.Slide

            // A 4px gap under the strip, as StelePopout leaves.
            implicitWidth: plate.width
            implicitHeight: plate.height + 4

            // The pointer has left both the cell and the popup: 600ms of grace
            // for the hand-off between two surfaces, then it closes.
            Timer {
                interval: 600
                running: !cellHover.hovered && !plateHover.hovered && root.hotRow === null
                onTriggered: root.open = false
            }

            Rectangle {
                id: plate
                y: 4
                width: 208
                height: rows.implicitHeight + 12
                radius: 0
                color: root.withA(root.livery.barBg, 0.96)
                border.width: 1
                border.color: root.livery.barAccent

                HoverHandler { id: plateHover }

                Column {
                    id: rows
                    x: 1
                    y: 6
                    width: parent.width - 2

                    PickRow {
                        live: false
                        ink: 0.7
                        label: root.answer
                               ? (root.answer.ok ? (root.answer.song || "no song") + " · " + root.mode : root.mode)
                               : "…"
                    }
                    PickRow {
                        visible: !root.answer || !root.answer.ok || root.drafts.length === 0
                        live: false
                        ink: 0.55
                        label: !root.answer ? "…"
                               : (root.answer.ok ? "no saved drafts" : "" + (root.answer.message || "no answer"))
                    }
                    Repeater {
                        model: root.drafts
                        PickRow {
                            required property var modelData
                            label: "" + modelData.name
                            marked: !!modelData.current
                            onActivated: {
                                if (modelData.current) root.open = false
                                else root.act("enter", "" + modelData.name)
                            }
                        }
                    }
                    PickRow {
                        label: "+ new draft"
                        onActivated: root.act("new")
                    }
                    PickRow {
                        visible: root.mode === "staging"
                        label: "save current as draft"
                        onActivated: root.act("save")
                    }
                }
            }
        }
    }
}
