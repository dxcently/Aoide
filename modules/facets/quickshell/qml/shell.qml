// shell.qml — Aoide Quickshell root.
//
// Entry point for the Quickshell session. Instantiates all surface widgets
// and wires the shared note loader (NoteState) so every widget hot-reloads
// from song/stage/notes.json when it changes.
//
// Communication discipline (CONTRACTS.md / entities/Quickshell):
//   - Reads state files from song/stage/ (NoteState watches notes.json).
//   - Issues commands to shellbridge via unix socket (ShellBridge singleton).
//   - Never speaks MCP or any agent protocol.
//
// Layer-shell wrappers (v1): the surface widgets are self-contained Items;
// shell.qml wraps the ones that own screen real estate in PanelWindows so
// they register real Wayland layer surfaces (the wrapper is the "shared
// layer-shell typing" the widget headers anticipate). Each widget still
// fills `parent` (the window content), so the files stay swappable Items.

import QtQuick
import QtQml.Models
import Quickshell
import Quickshell.Io
import Quickshell.Wayland

ShellRoot {
    // ── Shared singletons (one instance for the whole session) ─────────────
    NoteState { id: notes }
    ShellBridge { id: bridge }

    // ── Shared session state (floating gadgets + DAG trace link) ───────────
    // A plain QtObject passed by property, exactly like notes/bridge — the
    // cleanest quickshell idiom for cross-widget state that needs no file
    // watch of its own. Holds: (a) `floatingModel`, the ListModel of gadgets
    // torn off onto DesktopGadgets ({ kind, gx, gy }); and (b) `tracedSessionId`,
    // the hover-trace link — TerminalManagerGadget writes it on row hover,
    // DagGraphGadget highlights the node whose id matches. Session-scoped, no
    // persistence (v1). QtObject has no default property, so the ListModel is a
    // named property (the NoteState.noteFile idiom).
    QtObject {
        id: shared

        property ListModel floatingModel: ListModel {}
        property string tracedSessionId: ""

        // Cascading seed position so successive tear-offs don't stack exactly.
        function floatGadget(kind) {
            var base = 360
            var step = 34
            var n = floatingModel.count
            floatingModel.append({
                "kind": kind,
                "gx": base + (n % 4) * step,
                "gy": 80 + (n % 4) * step
            })
        }
        function unfloatGadget(index) {
            if (index >= 0 && index < floatingModel.count)
                floatingModel.remove(index)
        }
    }

    // ── Wallpaper (background layer, full screen, click-through) ───────────
    // Solid palette-bg fallback until a cover is staged; sits beneath every
    // window. Empty input mask → never intercepts desktop clicks.
    PanelWindow {
        id: wallpaperWin
        anchors { top: true; bottom: true; left: true; right: true }
        exclusiveZone: 0
        color: "transparent"
        WlrLayershell.layer: WlrLayer.Background
        WlrLayershell.namespace: "aoide-wallpaper"
        mask: Region { width: 0; height: 0 }

        AoideWallpaper {
            anchors.fill: parent
            notes: notes
        }
    }

    // ── Bar (top edge, exclusive — reserves its height) ────────────────────
    PanelWindow {
        id: barWin
        anchors { top: true; left: true; right: true }
        implicitHeight: bar.implicitHeight
        exclusiveZone: bar.stripHeight
        color: "transparent"
        WlrLayershell.layer: WlrLayer.Top
        // Distinct namespace → target of the compositor facet's glass
        // layerrule (blur behind the translucent barBg — dxflake's
        // "namespace waybar" posture, aoide-native name).
        WlrLayershell.namespace: "aoide-bar"

        AoideBar {
            id: bar
            anchors.fill: parent
            notes: notes
            bridge: bridge
        }
    }

    // ── Gadget dock (surface #8, agentWidgets) ─────────────────────────────
    // Win7-sidebar homage: LEFT-edge PINNABLE POPUP of ASCII-chromed gadgets on
    // Aero-glass. Full-height, non-exclusive, TOP layer so the hot-edge hover
    // reaches it above tiled windows. The input mask is the thin hot strip
    // when closed and the whole window while shown — so the retracted drawer
    // never deadens the left edge of the screen. Contains the DAG gadget →
    // this popup is the primary DAG affordance.
    PanelWindow {
        id: dockWin
        anchors { top: true; bottom: true; left: true }
        implicitWidth: dock.dockWidth + 2 * dock.edgeMargin
        exclusiveZone: 0
        color: "transparent"
        WlrLayershell.layer: WlrLayer.Top
        WlrLayershell.namespace: "aoide-dock"

        mask: Region {
            // Always-live hot strip.
            Region { x: 0; y: 0; width: dock.hotEdgeWidth; height: dockWin.height }
            // Whole window only while the drawer is shown (zero-width = off).
            Region {
                x: 0; y: 0
                width: dock.shown ? dockWin.width : 0
                height: dockWin.height
            }
        }

        AoideAgentWidgets {
            id: dock
            anchors.fill: parent
            notes: notes
            bridge: bridge
            shared: shared
        }
    }

    // ── Desktop gadget layer (Win7 tear-off) ───────────────────────────────
    // Floating copies of dock gadgets, dragged out onto the desktop. Full-
    // screen non-exclusive Top layer; its input mask is the union of the
    // floating gadgets' rects (see DesktopGadgets.qml) so bare desktop stays
    // click-through. Namespace "aoide-gadgets" — verify with `hyprctl layers`.
    DesktopGadgets {
        notes: notes
        bridge: bridge
        shared: shared
    }

    // ── Overlay skeletons (load clean; each gains its own PanelWindow wrapper
    // in a later pass). Hidden/dormant by default; kept wired to notes/bridge
    // so the swap seam stays intact.
    //   - AoideNotifications : notification stack (bridge-fed)
    //   - AoideLauncher      : SUPER+Space launcher (bridge-toggled)
    //   - AoideOsd           : volume/brightness OSD
    //   - AoideLockscreen    : ext-session-lock surface
    //   - AoideGreeter       : greetd greeter surface
    //   - AoideSessionGraph  : DORMANT full-screen DAG overlay (no keybind;
    //     the dock above holds the live DAG gadget).
    AoideNotifications { notes: notes; bridge: bridge }
    AoideLauncher { notes: notes; bridge: bridge }
    AoideOsd { notes: notes }
    AoideLockscreen { notes: notes }
    AoideGreeter { notes: notes }
    AoideSessionGraph { notes: notes; bridge: bridge }
}
