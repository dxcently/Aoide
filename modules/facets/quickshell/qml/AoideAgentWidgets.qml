// AoideAgentWidgets.qml — the gadget dock (surface #8, "agentWidgets").
//
// Design mapping (rice intent — Windows 7 + ASCII note theming): Win7 *sidebar
// gadgets* → an Aoide right-edge gadget dock. A vertical stack of gadgets, each
// framed with box-drawing chrome (╔═[ TITLE ]═╗ title bar, ║ side rails, ╚═╝
// footer) and rendered on Aero-glass — a semi-transparent paletteBg panel that
// reads as frosted glass because the compositor facet enables Hyprland blur
// (blur size 8, passes 3; see modules/facets/compositor + liner intent). ALL
// colors come from notes (paletteBg/Fg/Accent/Urgent + bar/notif tier reads as
// NoteState exposes them); the ASCII chrome is monospace. Zero hardcoded hex.
//
// Layer / anchoring (precedent: AoideWallpaper + AoideOsd):
//   The skeleton surfaces are plain Items hosted directly under ShellRoot in
//   shell.qml — no explicit Quickshell PanelWindow/WlrLayershell wrapper is used
//   yet (layer-shell typing is a v1 concern shared across every surface). Like
//   AoideWallpaper (a desktop-layer, non-interactive backdrop) and AoideOsd /
//   AoideSessionGraph (overlays anchored WITHIN their parent that never reserve
//   space), this dock anchors itself to the RIGHT EDGE inside `parent` and does
//   NOT reserve/exclude space the way the bar would — Win7 gadgets sit ON the
//   desktop, over the wallpaper, under windows. When surfaces gain real
//   layer-shell wrappers (v1), this dock should claim a non-exclusive
//   background/bottom layer, mirroring whatever AoideWallpaper adopts.
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

    // ── Dock geometry (v0 local defaults; note reads in v1) ────────────────
    readonly property int dockWidth: 300
    readonly property int edgeMargin: 8
    readonly property real glassOpacity: 0.72   // Aero-glass over compositor blur

    // ── Right-edge anchored, non-exclusive (does NOT reserve space) ────────
    anchors.right: parent ? parent.right : undefined
    anchors.top: parent ? parent.top : undefined
    anchors.bottom: parent ? parent.bottom : undefined
    anchors.rightMargin: edgeMargin
    anchors.topMargin: edgeMargin
    anchors.bottomMargin: edgeMargin
    implicitWidth: dockWidth

    // ── Gadget stack ───────────────────────────────────────────────────────
    ColumnLayout {
        anchors.right: parent.right
        anchors.top: parent.top
        width: root.dockWidth
        spacing: 10

        // ── Gadget 1: TERMINALS (Terminal-Commander roster) ──────────────
        GadgetFrame {
            Layout.fillWidth: true
            notes: root.notes
            title: "TERMINALS"
            TerminalManagerGadget {
                width: parent.width
                notes: root.notes
                bridge: root.bridge
            }
        }

        // ── Gadget 2: DAG (compact always-on session graph) ──────────────
        GadgetFrame {
            Layout.fillWidth: true
            notes: root.notes
            title: "DAG"
            DagGraphGadget {
                width: parent.width
                notes: root.notes
                bridge: root.bridge
            }
        }

        // ── Gadget 3: CLOCK (Win7-clock homage) ──────────────────────────
        GadgetFrame {
            Layout.fillWidth: true
            notes: root.notes
            title: "CLOCK"
            ClockGadget {
                width: parent.width
                notes: root.notes
            }
        }

        // ── Gadget 4: METERS (CPU/RAM ASCII gauges) ──────────────────────
        GadgetFrame {
            Layout.fillWidth: true
            notes: root.notes
            title: "METERS"
            MeterGadget {
                width: parent.width
                notes: root.notes
            }
        }
    }
}
