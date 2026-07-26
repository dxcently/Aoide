// shell.qml — Aoide Quickshell root.
//
// Entry point for the Quickshell session. Instantiates all surface widgets
// and wires the shared token loader (TokenState singleton) so every widget
// hot-reloads from song/stage/tokens.json when it changes.
//
// Communication discipline (CONTRACTS.md / entities/Quickshell):
//   - Reads state files from song/stage/ (TokenState watches tokens.json).
//   - Issues commands to shellbridge via unix socket (ShellBridge singleton).
//   - Never speaks MCP or any agent protocol.

import QtQuick
import Quickshell
import Quickshell.Io

ShellRoot {
    // ── Shared singletons (one instance for the whole session) ─────────────
    TokenState { id: tokens }
    ShellBridge { id: bridge }

    // ── Surface widgets ────────────────────────────────────────────────────
    // Each widget is a separate file so Melete can swap them independently.
    // All read colors / typography / geometry from `tokens`; never hardcode.

    AoideBar {
        tokens: tokens
        bridge: bridge
    }

    AoideNotifications {
        tokens: tokens
        bridge: bridge
    }

    AoideLauncher {
        tokens: tokens
        bridge: bridge
    }

    AoideOsd {
        tokens: tokens
    }

    AoideLockscreen {
        tokens: tokens
    }

    AoideGreeter {
        tokens: tokens
    }

    AoideWallpaper {
        tokens: tokens
    }
}
