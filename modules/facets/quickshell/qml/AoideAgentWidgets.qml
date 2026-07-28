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
// from notes (paletteBg/Fg/Accent/Urgent + bar/notif tiers as DrachmaState
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
    // the popup within the grace window cancels the hide, so crossing the
    // strip→panel gap (or briefly overshooting an edge) never flaps the dock.
    // A generous 1.5 s grace so the dock lingers after a hover/selection
    // instead of snapping shut the instant the pointer drifts off.
    property bool shown: false

    Timer {
        id: graceTimer
        interval: 1500
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
        // Full-height container: the agent pair fills the upper space and the
        // ambient gadgets (meters/power/clock) are bottom-seated, so the slab
        // now spans the full screen (minus edge margins) instead of wrapping.
        height: parent.height - 2 * root.edgeMargin
        // CLIP the container: the DAG gadget's node labels / leader arcs can
        // overflow the panel's right edge; when the panel is parked off-screen
        // left that overflow bled back onto the wallpaper (12h25m, session ids,
        // a stray leader). Confine every child to the panel bounds.
        clip: true

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
        // A HoverHandler — NOT a MouseArea — tracks the pointer over the panel.
        // A pointer handler reports `hovered` even when a child gadget/row
        // MouseArea is under the cursor (a MouseArea would lose containsMouse to
        // the child, dropping overPanel and starting the close). So hovering
        // ANYTHING inside keeps the dock open; only a true exit arms the grace.
        HoverHandler {
            id: panelHover
            onHoveredChanged: root.overPanel = hovered
        }

        // ── Pantheon wireframe-depth stack (container body) ────────────────
        // Same recipe as GadgetFrame: two hollow offset outline copies (holoBlue,
        // base0D) behind the container glass. VANISHING-POINT direction: the dock
        // hugs the LEFT edge, so its back copies project RIGHT (toward screen
        // centre) and only SLIGHTLY down (depthDy 0.45) — a right-dominant stack.
        // Declared first → render behind the panel; the 10px panelCol margin
        // absorbs the offset so nothing clips. Border-only, no MouseArea.
        readonly property int depthOff1: 3
        readonly property int depthOff2: 6
        readonly property real depthDx: 1
        readonly property real depthDy: 0.45
        Rectangle {
            x: parent.depthOff2 * parent.depthDx; y: parent.depthOff2 * parent.depthDy
            width: parent.width; height: parent.height
            radius: 0
            color: "transparent"
            border.color: root.notes.holoBlue
            border.width: 1
            opacity: 0.18
        }
        Rectangle {
            x: parent.depthOff1 * parent.depthDx; y: parent.depthOff1 * parent.depthDy
            width: parent.width; height: parent.height
            radius: 0
            color: "transparent"
            border.color: root.notes.holoBlue
            border.width: 1
            opacity: 0.35
        }

        // ── The container body (khoa: the drawer IS a container) ───────────
        // One Aero-glass panel the two agent gadgets sit INSIDE — the Win7
        // sidebar reading. Glass + gloss + accent border, same language as
        // the bar strip and GadgetFrame; the compositor's aoide-dock blur
        // rule reads through the translucent body.
        Rectangle {
            anchors.fill: parent
            radius: 0
            color: root.notes.paletteBg
            opacity: root.glassOpacity
        }
        // Wireframe outline (front face) — crisp wireCyan hollow rule over glass.
        Rectangle {
            anchors.fill: parent
            radius: 0
            color: "transparent"
            border.color: root.notes.wireCyan
            border.width: 1
            opacity: 0.5
        }
        Rectangle {
            anchors.fill: parent
            radius: 0
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

            // ══ Header callout with pin affordance ═════════════════════════
            // The container's own Pantheon callout: `gadgets.case` on a short
            // leader (echoing GadgetFrame), with a lowercase dotted pin token at
            // the tail — `pin.on` when pinned, `pin.off` when peeking.
            Item {
                Layout.fillWidth: true
                implicitHeight: Math.max(headerText.implicitHeight, 12)

                Row {
                    anchors.left: parent.left
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 4

                    // Leader — anchor tick + rule + terminal dot (wireCyan dim).
                    Canvas {
                        id: headerLeader
                        anchors.verticalCenter: parent.verticalCenter
                        width: 13
                        height: 10
                        opacity: 0.6
                        onPaint: {
                            var ctx = getContext("2d")
                            ctx.reset(); ctx.clearRect(0, 0, width, height)
                            ctx.strokeStyle = notes.wireCyan; ctx.fillStyle = notes.wireCyan
                            ctx.lineWidth = 1
                            var cy = height / 2
                            ctx.beginPath(); ctx.moveTo(1, cy - 3); ctx.lineTo(1, cy + 3); ctx.stroke()
                            ctx.beginPath(); ctx.moveTo(1, cy); ctx.lineTo(width - 2, cy); ctx.stroke()
                            ctx.beginPath(); ctx.arc(width - 2, cy, 1.3, 0, 2 * Math.PI); ctx.fill()
                        }
                        Connections {
                            target: notes
                            function onWireCyanChanged() { headerLeader.requestPaint() }
                        }
                    }

                    Text {
                        id: headerText
                        anchors.verticalCenter: parent.verticalCenter
                        text: "gadgets.case"
                        color: notes.paletteFg
                        opacity: 0.6
                        font.family: "monospace"
                        font.pixelSize: 11
                    }
                }

                // Pin token — lowercase dotted state at the tail; click toggles.
                Text {
                    id: pinToken
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    text: root.pinned ? "pin.on" : "pin.off"
                    color: root.pinned ? notes.wireCyan : notes.paletteFg
                    opacity: root.pinned ? 0.85 : 0.5
                    font.family: "monospace"
                    font.pixelSize: 10
                }
                MouseArea {
                    width: pinToken.implicitWidth + 12
                    height: parent.height
                    anchors.right: parent.right
                    cursorShape: Qt.PointingHandCursor
                    onClicked: root.pinned = !root.pinned
                }
            }

            // ══ Gadget stack — agents on top, ambient widgets at the bottom
            // (v3 restructure) ══════════════════════════════════════════════
            // The dock now spans the full screen height. The top holds the
            // agent-facing gadgets — BATON (mini conductor), TERMINALS
            // (session roster) and DAG (session graph) — filling the upper
            // space via the slack spacer below them. The bottom holds the
            // ambient, non-agent gadgets — meters (cpu/gpu), power, clock —
            // in the same GadgetFrame chrome, seated flush at the container's
            // bottom edge. now-playing stays its own bar-spawned widget
            // (BarPopout.qml via AoideBar) — it did not move here.
            ColumnLayout {
                Layout.fillWidth: true
                Layout.fillHeight: true
                spacing: 10

                // ── Gadget 1: BATON (mini conductor / roster mini-view) ──
                GadgetFrame {
                    Layout.fillWidth: true
                    notes: root.notes
                    title: "baton.control"
                    floatable: true
                    onFloatRequested: root.shared.floatGadget("BATON")
                    BatonGadget {
                        width: parent.width
                        notes: root.notes
                        shared: root.shared
                    }
                }

                // ── Gadget 2: TERMINALS (Terminal-Commander roster) ──────
                GadgetFrame {
                    Layout.fillWidth: true
                    notes: root.notes
                    title: "terminals.roster"
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
                    title: "dag.trace"
                    floatable: true
                    onFloatRequested: root.shared.floatGadget("DAG")
                    DagGraphGadget {
                        width: parent.width
                        notes: root.notes
                        bridge: root.bridge
                        shared: root.shared
                    }
                }

                // Absorb slack so the agent pair tops out and the ambient
                // group below is pushed flush to the container's bottom.
                Item { Layout.fillWidth: true; Layout.fillHeight: true }

                // ── Gadget 4: METERS (cpu/gpu) — ambient, bottom-seated ──
                GadgetFrame {
                    Layout.fillWidth: true
                    notes: root.notes
                    title: "meters.pulse"
                    MeterGadget {
                        width: parent.width
                        notes: root.notes
                    }
                }

                // ── Gadget 5: POWER (reserve) — ambient, bottom-seated ───
                GadgetFrame {
                    Layout.fillWidth: true
                    notes: root.notes
                    title: "power.reserve"
                    PowerGadget {
                        width: parent.width
                        notes: root.notes
                    }
                }

                // (The clock/date live on the BAR, not the dock — no clock.face
                //  gadget here; meters + power are the dock's ambient pair.)

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
