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

    // ── Session-Graph (DAG) overlay ────────────────────────────────────────
    // Watches song/stage/graph.json; toggled via shellbridge (Hyprland keybind
    // execs `aoide shell graph toggle`). Row click → bridge.focusSession.
    AoideSessionGraph {
        notes: notes
        bridge: bridge
    }
}
