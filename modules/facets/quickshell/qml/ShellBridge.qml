// ShellBridge.qml — unix-socket client for the shellbridge daemon.
//
// Quickshell's side of the shellbridge IPC channel. All outbound commands
// (session-jump, rice preview triggers, etc.) go through here. Quickshell
// never calls hyprctl directly; it sends a command object to shellbridge and
// shellbridge dispatches to hyprctl.
//
// Protocol: newline-delimited JSON. Each command is a JSON object with a
// "cmd" field and payload fields. The socket path is the shellbridge default:
// $XDG_RUNTIME_DIR/aoide/shellbridge.sock (falls back to /run/user/<uid>/aoide/shellbridge.sock).
//
// Communication discipline: this is the ONLY outbound channel from QML.
// No MCP, no HTTP, no shell exec from QML — shellbridge is the gate.

import QtQuick
import Quickshell.Io

QtObject {
    id: root

    // ── Socket path ────────────────────────────────────────────────────────
    // shellbridge daemon writes its socket here. Quickshell connects on first
    // command; reconnects automatically on disconnect.
    readonly property string socketPath: {
        var runtimeDir = StandardPaths.writableLocation(StandardPaths.RuntimeLocation)
        return runtimeDir + "/aoide/shellbridge.sock"
    }

    // ── Session-jump stub ──────────────────────────────────────────────────
    // Backs the Terminal-Commander widget (concepts/Desktop-Architecture).
    // widget click → focusSession(address) → shellbridge → hyprctl dispatch
    //
    // Usage:  bridge.focusSession("0x55f1234abc")
    function focusSession(windowAddress) {
        sendCommand({
            cmd: "focuswindow",
            address: windowAddress
        })
    }

    // ── Generic command sender ─────────────────────────────────────────────
    function sendCommand(obj) {
        socket.sendTextMessage(JSON.stringify(obj) + "\n")
    }

    // ── Socket connection ──────────────────────────────────────────────────
    // STUB: SockClient is the placeholder type; replace with the correct
    // Quickshell unix-socket type once the Quickshell API is confirmed.
    // The structure (send as text, handle onConnected/onDisconnected) is stable.
    property var socket: QtObject {
        // Placeholder: wired to a real Quickshell IpcSocket in the full impl.
        function sendTextMessage(msg) {
            console.log("[aoide/shellbridge] STUB send:", msg)
        }
    }
}
