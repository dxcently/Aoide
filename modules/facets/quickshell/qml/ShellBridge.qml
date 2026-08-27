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
    // period. No payload: the daemon re-execs `aoide session reap` itself (see
    // shellbridge.rs's dispatch_recheck_sessions), whose atomic stage writes the
    // roster gadgets pick up through their own FileView watches. No hyprctl /
    // MCP / shell-exec from QML.
    function recheckSessions() {
        sendCommand({
            cmd: "rechecksessions"
        })
    }

    // ── Generic command sender ─────────────────────────────────────────────
    // Writes one newline-delimited JSON line to the shellbridge socket. EVERY
    // command takes the same queue → fresh-connect → flush path: the line is
    // queued, any stale link is dropped, a new connection is opened, and the
    // queue drains in onConnectedChanged. A persistent link was the original
    // posture, but a peer-closed socket does not reliably flip `connected`
    // back to false (Quickshell 0.3.0) — a zombie connection then eats clicks
    // silently (observed live: four reap clicks flushed in one burst, minutes
    // late). A unix-socket connect is cheap and every command here is a human
    // gesture, so a fresh link per command costs nothing and can never wedge.
    // This is the ONLY outbound path from QML (no hyprctl / MCP / shell exec).
    function sendCommand(obj) {
        socket._queue.push(JSON.stringify(obj) + "\n")
        socket.connected = false // drop any stale link before…
        socket.connected = true  // …reconnecting; the queue flushes on connect
    }

    // ── Socket connection ──────────────────────────────────────────────────
    // Quickshell.Io.Socket (0.3.0): a real unix-domain client. `connected` is
    // both the state and the trigger — sendCommand cycles it false→true per
    // command (see above), and the queue drains here once the link is up. If
    // the daemon is down the connect simply fails and the lines wait in
    // `_queue` for the next command's retry. shellbridge reads
    // newline-delimited JSON and dispatches (e.g. hyprctl focuswindow) — no
    // IPC is invented in QML.
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
