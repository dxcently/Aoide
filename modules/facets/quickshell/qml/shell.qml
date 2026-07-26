// shell.qml — Aoide Quickshell root.
//
// Entry point for the Quickshell session. Instantiates all surface widgets
// and wires the shared note loader (NoteState singleton) so every widget
// hot-reloads from song/stage/notes.json when it changes.
//
// Communication discipline (CONTRACTS.md / entities/Quickshell):
//   - Reads state files from song/stage/ (NoteState watches notes.json).
//   - Issues commands to shellbridge via unix socket (ShellBridge singleton).
//   - Never speaks MCP or any agent protocol.

import QtQuick
import Quickshell
import Quickshell.Io

ShellRoot {
    // ── Shared singletons (one instance for the whole session) ─────────────
    NoteState { id: notes }
    ShellBridge { id: bridge }

    // ── Surface widgets ────────────────────────────────────────────────────
    // Each widget is a separate file so Melete can swap them independently.
    // All read colors / typography / geometry from `notes`; never hardcode.

    AoideBar {
        notes: notes
        bridge: bridge
    }

    AoideNotifications {
        notes: notes
        bridge: bridge
    }

    AoideLauncher {
        notes: notes
        bridge: bridge
    }

    AoideOsd {
        notes: notes
    }

    AoideLockscreen {
        notes: notes
    }

    AoideGreeter {
        notes: notes
    }

    AoideWallpaper {
        notes: notes
    }

    // ── Gadget dock (surface #8, agentWidgets) ─────────────────────────────
    // Win7-sidebar homage: LEFT-edge PINNABLE POPUP of ASCII-chromed gadgets on
    // Aero-glass (translucent paletteBg over compositor blur). Hidden by
    // default; slides in on mouse hot-edge hover (pure QML) or on SUPER+G
    // (shellbridge → dockToggle, open-and-pin). Non-exclusive (reserves no
    // space) — sits over the wallpaper. Contains the DAG gadget → this popup is
    // the primary DAG affordance.
    AoideAgentWidgets {
        notes: notes
        bridge: bridge
    }

    // ── Session-Graph (DAG) overlay ────────────────────────────────────────
    // DORMANT: standalone bridge-only overlay watching song/stage/graph.json.
    // It has NO keybind now — SUPER+G summons the dock popup above (which holds
    // the DAG gadget). Kept intact for a future dedicated full-screen DAG view;
    // toggled only via shellbridge if a bind is re-added. Row click →
    // bridge.focusSession.
    AoideSessionGraph {
        notes: notes
        bridge: bridge
    }
}
