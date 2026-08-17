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

    // ── Session-jump ───────────────────────────────────────────────────────
    // Backs the Terminal-Commander / Conductor roster (concepts/Desktop-
    // Architecture). A row click sends the SESSION ID; the daemon resolves it to
    // the session's windowAddress (focus the exact window) or, if that isn't
    // resolved yet, to its workspace (switch there). QML never holds or sends an
    // address here — the bridge is the source of truth for id→window.
    //
    // Usage:  bridge.focusSession("conduct-1944-1785384887")
    function focusSession(sessionId) {
        sendCommand({
            cmd: "focussession",
            sessionId: sessionId
        })
    }

    // Focus a bare window by address — for surfaces that already hold a live
    // Hyprland address and no session id (e.g. a plain, untracked terminal).
    // Same socket gate; no hyprctl/MCP/shell-exec from QML.
    function focusWindow(address) {
        sendCommand({
            cmd: "focuswindow",
            address: address
        })
    }

    // ── Rice-mode toggle ───────────────────────────────────────────────────
    // Backs the bar's mode cell (bar.qml, rightContent's modeText). A click
    // is a two-way toggle, not a picker: no payload — the daemon reads
    // stage/mode.json itself and decides staging⇄declarative (see
    // shellbridge.rs's dispatch_rice_mode_toggle).
    function toggleRiceMode() {
        sendCommand({
            cmd: "ricemode"
        })
    }

    // ── Usage refresh ──────────────────────────────────────────────────────
    // Backs the CLAUDE ledger gadget's ❋ spark (UsageGadget.qml). A click asks
    // the daemon to re-run `aoide usage` NOW instead of waiting for the poller
    // timer: no payload — the daemon runs the command itself and atomic-writes
    // state/usage.json, which the gadget's own FileView watch picks up (see
    // shellbridge.rs's dispatch_usage_refresh). No hyprctl/shell-exec from QML.
    function refreshUsage() {
        sendCommand({
            cmd: "refreshusage"
        })
    }

    // ── Session recheck ────────────────────────────────────────────────────
    // Backs the Terminals/Conductor header recheck control. A click asks the
    // daemon to run the liveness/rehook sweep NOW — reap dead sessions (a
    // window/process that exited), decay `stopped` → `idle`, and prune orphaned
    // hook records — instead of waiting up to a full ~12s aoide-graph-reap.timer
    // period. No payload: the daemon re-execs `aoide graph reap` itself (see
    // shellbridge.rs's dispatch_recheck_sessions), whose atomic stage writes the
    // roster gadgets pick up through their own FileView watches. No hyprctl /
    // MCP / shell-exec from QML.
    function recheckSessions() {
        sendCommand({
            cmd: "rechecksessions"
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
