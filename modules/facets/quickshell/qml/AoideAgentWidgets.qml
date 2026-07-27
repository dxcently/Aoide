// AoideAgentWidgets.qml — the gadget dock (surface #8, "agentWidgets").
//
// v1 REDESIGN: LEFT-edge *pinnable popup*. Was a right-edge, always-visible
// column; now a hidden-by-default drawer that slides in from the LEFT screen
// edge on (a) mouse hot-edge hover and (b) a keybind (SUPER+G → shellbridge →
// dockOpen). Pinnable via an ASCII pin affordance in the header chrome. The
// gadget stack + Win7-ASCII chrome (frames, gadgets) are UNCHANGED — only the
// geometry glue and the reveal/hide state machine are new.
//
// Design mapping (rice intent — Windows 7 + ASCII note theming): Win7 *sidebar
// gadgets* → an Aoide gadget dock. A vertical stack of gadgets, each framed
// with box-drawing chrome (╔═[ TITLE ]═╗ title bar, ║ side rails, ╚═╝ footer)
// on Aero-glass — a semi-transparent paletteBg panel that reads as frosted
// glass because the compositor facet enables Hyprland blur (blur size 8,
// passes 3; see modules/facets/compositor + liner intent). ALL colors come
// from notes (paletteBg/Fg/Accent/Urgent + bar/notif tiers as NoteState
// exposes them); the ASCII chrome is monospace. Zero hardcoded hex.
//
// ── Reveal / hide state machine ─────────────────────────────────────────────
// Three logical states drive the panel's x-position (slide via a Behavior):
//
//   STATE     | condition                                    | panel
//   ----------|----------------------------------------------|--------
//   hidden    | !pinned && !hotEdge && !overPanel            | off-screen
//   peeking   | !pinned && (hotEdge || overPanel)            | shown (hover)
//   pinned    | pinned  (bridge/keybind or [pin] clicked)    | shown (sticky)
//
// Derived open predicate:  open = pinned || hotEdge || overPanel
//   - hotEdge   : pointer inside the thin left hot strip (always-present).
//   - overPanel : pointer anywhere over the popup (the hot strip + popup are
//                 ONE hover union, so hovering rows / clicking gadgets never
//                 counts as "left the popup").
//   - pinned    : sticky; survives the pointer leaving entirely.
//
// Transition detail (auto-hide grace): when NOT pinned and the pointer leaves
// BOTH the hot strip and the popup, a ~400 ms grace timer arms; if the pointer
// re-enters either region before it fires, the timer cancels and the panel
// stays. This keeps crossing the ~gap between strip and panel from flapping.
// Pinned short-circuits the timer entirely.
//
// Keybind (SUPER+G) → shellbridge → dockOpen(): opens AND pins. A keybind that
// merely peeked would instantly auto-hide once the pointer settled, so the
// bridge/keybind path is defined as open-and-pin (see dockShow/dockToggle).
// Bridge wiring is stub-level today (the shellbridge socket loop is stubbed),
// exactly like AoideLauncher's `visible_` / AoideSessionGraph's `visible_`.
//
// Layer / anchoring (precedent: AoideWallpaper + AoideOsd):
//   The skeleton surfaces are plain Items hosted directly under ShellRoot in
//   shell.qml — no explicit Quickshell PanelWindow/WlrLayershell wrapper is
//   used yet (layer-shell typing is a v1 concern shared across every surface).
//   Like AoideWallpaper / AoideOsd, this dock anchors WITHIN `parent` and does
//   NOT reserve/exclude space (non-exclusive) — Win7 gadgets sit ON the
//   desktop, over the wallpaper, under windows. When surfaces gain real
//   layer-shell wrappers (v1), this dock should claim a non-exclusive
//   top/overlay layer so the hot-edge hover reaches it above tiled windows.
//
// Geometry stays v0 defaults (no geometry note tier yet) — width/margins are
// local constants, to become note reads in v1 (same posture as AoideBar).

import QtQuick
import QtQuick.Layouts
import Quickshell.Io

Item {
    id: root

    // ── Note + bridge dependencies (injected by shell.qml) ─────────────────
    required property var notes
    required property var bridge
    // Shared session state (floating gadgets + DAG trace link). Each frame's
    // [↗] tear-off calls shared.floatGadget(kind); the trace link rides through
    // to the terminal/DAG gadgets below.
    required property var shared

    // ── Dock geometry (v0 local defaults; note reads in v1) ────────────────
    readonly property int dockWidth: 300
    readonly property int edgeMargin: 8
    readonly property int hotEdgeWidth: 5        // thin always-present hover strip
    readonly property real glassOpacity: 0.72    // Aero-glass over compositor blur
    readonly property int slideMs: 150           // Win7-vibe reveal animation

    // ── Fill the parent so the hot strip spans the full screen height ──────
    // Non-exclusive: this Item reserves NO space (no anchors.margins on parent,
    // no layer-shell exclusive zone) — it merely overlays `parent`.
    anchors.fill: parent ? parent : undefined

    // ── State machine inputs ───────────────────────────────────────────────
    property bool pinned: false      // sticky (bridge/keybind or [pin] click)
    property bool hotEdge: false     // pointer inside the left hot strip
    property bool overPanel: false   // pointer anywhere over the popup

    // Derived: is the popup logically open? (peeking OR pinned)
    readonly property bool dockOpen: pinned || hotEdge || overPanel

    // ── Bridge-driven entry points (stub-level; mirror AoideLauncher) ──────
    // shellbridge will send { cmd: "dock", action: "show|hide|toggle" } and
    // call one of these. Keybind/bridge opens ALWAYS pin (see header note).
    function dockShow()   { pinned = true }
    function dockHide()   { pinned = false }   // falls back to hover peek
    function dockToggle() { pinned = !pinned }

    // ── Auto-hide grace timer ──────────────────────────────────────────────
    // `shown` is what the panel's x actually binds to. Opening is immediate
    // (dockOpen → shown right away); closing is GATED by the timer: when the
    // hover union drops while unpinned, the timer arms and only its firing —
    // with dockOpen still false — retracts the panel. Re-entering the strip or
    // the popup within the ~400 ms window cancels the hide, so crossing the
    // strip→panel gap (or briefly overshooting an edge) never flaps the dock.
    property bool shown: false

    Timer {
        id: graceTimer
        interval: 400
        repeat: false
        onTriggered: {
            if (!root.dockOpen)
                root.shown = false
        }
    }

    onDockOpenChanged: {
        if (dockOpen) {
            graceTimer.stop()
            shown = true
        } else {
            graceTimer.restart()
        }
    }

    // ══ Left hot-edge strip — always present, pure QML (works live) ════════
    // A thin full-height MouseArea pinned to the LEFT edge. Hovering it flips
    // hotEdge → dockOpen → panel slides in. hoverEnabled so containsMouse
    // tracks without a click.
    MouseArea {
        id: hotStrip
        anchors.left: parent.left
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        width: root.hotEdgeWidth
        hoverEnabled: true
        acceptedButtons: Qt.NoButton   // hover-only; never steals clicks
        onContainsMouseChanged: root.hotEdge = containsMouse
    }

    // ══ The sliding popup panel ════════════════════════════════════════════
    // Anchored logically to the left; slides via x. Hidden: x = -width - margin
    // (fully off-screen). Shown: x = edgeMargin. Behavior animates the reveal.
    Item {
        id: panel
        y: root.edgeMargin
        width: root.dockWidth
        // Content-sized container: with only the agent pair inside, a
        // full-height slab reads as empty glass — the container wraps its
        // two widgets instead (clamped to the screen when content grows).
        height: Math.min(parent.height - 2 * root.edgeMargin,
                         panelCol.implicitHeight + 20)

        // Slide reveal: off-screen when closed, edgeMargin when open.
        x: root.shown ? root.edgeMargin
                      : -(root.dockWidth + root.edgeMargin)
        Behavior on x {
            NumberAnimation {
                duration: root.slideMs
                easing.type: Easing.OutCubic
            }
        }

        // ── Popup hover union: a NoButton hoverEnabled catcher covering the
        // whole panel. acceptedButtons: Qt.NoButton → clicks fall THROUGH to
        // the gadgets/rows/pin beneath, but containsMouse still tracks the
        // pointer anywhere over the popup. Placed FIRST (lowest z) so the pin
        // button and gadget MouseAreas sit above it and receive their clicks.
        MouseArea {
            id: panelHover
            anchors.fill: parent
            hoverEnabled: true
            acceptedButtons: Qt.NoButton
            onContainsMouseChanged: root.overPanel = containsMouse
        }

        // ── The container body (khoa: the drawer IS a container) ───────────
        // One Aero-glass panel the two agent gadgets sit INSIDE — the Win7
        // sidebar reading. Glass + gloss + accent border, same language as
        // the bar strip and GadgetFrame; the compositor's aoide-dock blur
        // rule reads through the translucent body.
        Rectangle {
            anchors.fill: parent
            radius: 6
            color: root.notes.paletteBg
            opacity: root.glassOpacity
            border.color: root.notes.paletteAccent
            border.width: 1
        }
        Rectangle {
            anchors.fill: parent
            radius: 6
            gradient: Gradient {
                GradientStop { position: 0.0;  color: Qt.rgba(1, 1, 1, 0.10) }
                GradientStop { position: 0.42; color: Qt.rgba(1, 1, 1, 0.03) }
                GradientStop { position: 0.5;  color: Qt.rgba(1, 1, 1, 0.00) }
                GradientStop { position: 1.0;  color: Qt.rgba(1, 1, 1, 0.03) }
            }
        }

        ColumnLayout {
            id: panelCol
            anchors.fill: parent
            anchors.margins: 10
            spacing: 10

            // ══ Header chrome with pin affordance ══════════════════════════
            // Matches GadgetFrame's box-drawing language: a double-rule title
            // bar with a [pin] slot. Glyph: [■] pinned / [+] unpinned.
            //   ╔═[ GADGETS ]══…══[■]═╗   (pinned)
            //   ╔═[ GADGETS ]══…══[+]═╗   (unpinned)
            Item {
                Layout.fillWidth: true
                implicitHeight: headerText.implicitHeight

                // Fill-dashes are computed in JS so the ╗ lands flush right and
                // the [pin] slot sits just before it (mirrors GadgetFrame.titleLine).
                readonly property int chromePx: 12
                readonly property real cellW: chromePx * 0.6
                readonly property int cols: Math.max(16, Math.floor(width / cellW))
                readonly property string pinGlyph: root.pinned ? "[■]" : "[+]"
                function headerLine(cols) {
                    var head = "╔═[ GADGETS ]"
                    var tail = pinGlyph + "═╗"
                    var fillCount = cols - head.length - tail.length
                    if (fillCount < 0) fillCount = 0
                    var fill = ""
                    for (var i = 0; i < fillCount; i++) fill += "═"
                    return head + fill + tail
                }

                Text {
                    id: headerText
                    width: parent.width
                    text: parent.headerLine(parent.cols)
                    color: notes.paletteAccent
                    font.family: "monospace"
                    font.pixelSize: parent.chromePx
                    font.bold: true
                    clip: true
                }

                // Click target over the [pin] slot at the right end. Sized to a
                // few cells; toggles the sticky pin. Sits above panelHover.
                MouseArea {
                    width: 4 * parent.cellW
                    height: parent.height
                    anchors.right: parent.right
                    cursorShape: Qt.PointingHandCursor
                    onClicked: root.pinned = !root.pinned
                }
            }

            // ══ Gadget stack — the AGENT pair only (v2 restructure) ════════
            // The dock is a container for the two agent-facing gadgets:
            // TERMINALS (session roster) and DAG (session graph). Every other
            // gadget (now-playing, power, calendar, clock, meters) is its own
            // widget spawned from a bar cell (BarPopout.qml via AoideBar).
            ColumnLayout {
                Layout.fillWidth: true
                Layout.fillHeight: true
                spacing: 10

                // ── Gadget 1: BATON (mini conductor / roster mini-view) ──
                GadgetFrame {
                    Layout.fillWidth: true
                    notes: root.notes
                    title: "BATON"
                    floatable: true
                    onFloatRequested: root.shared.floatGadget("BATON")
                    BatonGadget {
                        width: parent.width
                        notes: root.notes
                    }
                }

                // ── Gadget 2: TERMINALS (Terminal-Commander roster) ──────
                GadgetFrame {
                    Layout.fillWidth: true
                    notes: root.notes
                    title: "TERMINALS"
                    floatable: true
                    onFloatRequested: root.shared.floatGadget("TERMINALS")
                    TerminalManagerGadget {
                        width: parent.width
                        notes: root.notes
                        bridge: root.bridge
                        shared: root.shared
                    }
                }

                // ── Gadget 3: DAG (compact always-on session graph) ──────
                GadgetFrame {
                    Layout.fillWidth: true
                    notes: root.notes
                    title: "DAG"
                    floatable: true
                    onFloatRequested: root.shared.floatGadget("DAG")
                    DagGraphGadget {
                        width: parent.width
                        notes: root.notes
                        bridge: root.bridge
                        shared: root.shared
                    }
                }

                // Absorb slack so the stack tops out cleanly.
                Item { Layout.fillWidth: true; Layout.fillHeight: true }

                // ── Dock footer rule: staff run with clef (ornament vocab,
                // verbatim — trailing U+3164 hangul filler included). The
                // header keeps its computed ═ math; this standalone rule is
                // the dock's one footer ornament seam.
                Text {
                    Layout.alignment: Qt.AlignHCenter
                    text: "𝄂𝄚𝅦𝄚𝄞𝅄ㅤ"
                    color: notes.paletteAccent
                    opacity: 0.5
                    font.family: "monospace"
                    font.pixelSize: 13
                }
            }
        }
    }
}
