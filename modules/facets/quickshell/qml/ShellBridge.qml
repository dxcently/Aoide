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
import Quickshell
import Quickshell.Io

QtObject {
    id: root

    // ── Socket path ────────────────────────────────────────────────────────
    // shellbridge daemon writes its socket here. Quickshell connects on first
    // command; reconnects automatically on disconnect.
    readonly property string socketPath: {
        var runtimeDir = Quickshell.env("XDG_RUNTIME_DIR")
        if (!runtimeDir || runtimeDir.length === 0)
            runtimeDir = "/run/user/" + Quickshell.env("UID")
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
    // Writes one newline-delimited JSON line to the shellbridge socket. If the
    // socket is up, it goes out immediately; otherwise the line is queued and
    // the connection is initiated — the queue flushes on connect. This is the
    // ONLY outbound path from QML (no hyprctl / MCP / shell exec here).
    function sendCommand(obj) {
        var line = JSON.stringify(obj) + "\n"
        if (socket.connected) {
            socket.write(line)
            socket.flush()
        } else {
            socket._queue.push(line)
            socket.connected = true // initiate connect; _queue flushes on connect
        }
    }

    // ── Socket connection ──────────────────────────────────────────────────
    // Quickshell.Io.Socket (0.3.0): a real unix-domain client. `connected` is
    // both the state and the trigger — set it true to connect. We connect on
    // demand and stay connected; on any drop, `connected` goes false and the
    // next sendCommand reconnects. shellbridge reads newline-delimited JSON and
    // dispatches (e.g. hyprctl focuswindow) — no IPC is invented in QML.
    property Socket socket: Socket {
        path: root.socketPath
        connected: false

        // Lines queued while disconnected; drained once the link comes up.
        property var _queue: []

        onConnectedChanged: {
            if (connected) {
                while (_queue.length > 0)
                    write(_queue.shift())
                flush()
            }
        }
    }
}
