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
// Every command is fire-and-forget except "sessionaction", "projectaction",
// "workspaceaction", "sessiontrace", "ricemenu" and "ricedeclare", which are answered with
// one JSON line on the same connection, which then closes.
//
// Communication discipline: this is the ONLY outbound channel from QML.
// No MCP, no HTTP, no shell exec from QML — shellbridge is the gate.

import QtQuick
import Quickshell
import Quickshell.Io

QtObject {
    id: root

    // ── Socket path ────────────────────────────────────────────────────────
    // shellbridge daemon writes its socket here. Every command connects to it
    // fresh, over a socket of its own (see sendCommand and actionFactory).
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

    // ── Acknowledged session actions ───────────────────────────────────────
    // The ONE command that answers. The daemon re-execs the matching
    // `aoide session …` through aoided and writes one JSON line back on the
    // same connection, which it then closes.
    //
    //   bridge.sessionAction(id, "project",       { project: "aoide" }, cb)
    //   bridge.sessionAction(id, "project",       { project: "" },      cb)  // clear
    //   bridge.sessionAction(id, "kill",          {},                   cb)
    //   bridge.sessionAction(id, "undying",       { state: "on" },      cb)
    //   bridge.sessionAction(id, "createproject", { name, paths: [] },  cb)
    //   bridge.sessionAction(id, "editproject",   { name, paths: [] },  cb)
    //
    // `cb` is called EXACTLY ONCE with { ok, message } (the reply also echoes
    // `action` and `sessionId` so a caller can correlate, carries
    // `partial: true` when a multi-step action changed something and then
    // failed, and carries the CLI outcome's `data` verbatim when there is
    // one — a kill names the terminal and pid it stops there). Each call gets its
    // own throwaway socket, like sendCommand, and no line is ever replayed. A
    // reply that does not arrive within replyTimeoutMs fires cb({ ok: false, … })
    // once and the socket is destroyed. A timed-out action may still have
    // executed; the caller re-reads the roster, it never re-sends.
    readonly property int replyTimeoutMs: 5000

    function sessionAction(sessionId, action, fields, callback) {
        _ask({
            cmd: "sessionaction",
            sessionId: "" + sessionId,
            action: "" + action,
            fields: fields || ({})
        }, callback, { ok: false, message: "shellbridge client unavailable" })
    }

    // The shape every answered verb shares: one throwaway socket, the `wanted`
    // line out, one JSON line back to `callback`. `unavailable` is what the
    // callback gets when no connection could be made at all.
    function _ask(wanted, callback, unavailable) {
        var conn = actionFactory.createObject(root, {
            path: root.socketPath,
            cb: callback || null,
            wanted: wanted
        })
        if (!conn) {
            if (callback)
                callback(unavailable)
            return
        }
        conn.start()
    }

    // ── Workspace actions (bind a workspace to a project) ─────────────────
    // The same one-shot socket as sessionAction: one `workspaceaction` line
    // out, one JSON line back, no replay. `fields` carries the wire
    // keys verbatim — `action` ("set" | "clear"), and optionally `workspace`
    // (an id; absent = the focused one), `project`, `new` (create a name-only
    // project first):
    //
    //   bridge.workspaceAction({ action: "set", workspace: 3, project: "aoide" }, cb)
    //   bridge.workspaceAction({ action: "set", project: "x", new: true },      cb)
    //   bridge.workspaceAction({ action: "clear", workspace: 3 },               cb)
    //
    // `cb` is called EXACTLY ONCE with the daemon's reply { ok, message, action,
    // workspace, project?, data? } — `reason: "no-compositor"` when there is no
    // compositor — or { ok: false, message } on a timeout or an unreachable
    // socket.
    function workspaceAction(fields, callback) {
        var f = fields || ({})
        var wanted = { cmd: "workspaceaction", action: "" + (f.action || "") }
        if (f.workspace !== undefined && f.workspace !== null)
            wanted.workspace = f.workspace
        if (f.project !== undefined && f.project !== null)
            wanted.project = "" + f.project
        if (f.new === true)
            wanted.new = true
        _ask(wanted, callback, { ok: false, message: "shellbridge client unavailable" })
    }

    // ── The read-only trace query ──────────────────────────────────────────
    // The ONE window onto a session's emitted output: what it has THOUGHT,
    // SAID, called and got back, projected by `aoide session trace --json`'s
    // `data.steps` (docs/architecture/EIDOLON-TRACE.md). Read-only — the daemon
    // resolves, reads and answers; it writes nothing.
    //
    //   bridge.traceSession(id, 12, "line",   cb)   // a card face
    //   bridge.traceSession(id, 24, "detail", cb)   // the Details page
    //
    // `cb` is called EXACTLY ONCE with the daemon's own answer:
    // { ok: true, sessionId, clip, lines, trace, at, steps: [...], stepsOmitted }
    // or { ok: false, sessionId, clip, lines, reason, message } — `message` is
    // the CLI's own taught refusal verbatim ("no readable trace at …"), and
    // `reason` its machine name (no-trace / no-presence / unknown-agent /
    // remote-target / not-found / ambiguous / no-answer), so a caller can tell
    // a refusal from a session that emitted nothing.
    //
    // Each step is { id, ts, kind, text, error, clipped } with `kind` in
    // thinking / say / tool / result / settled / user / other — `text` is the
    // harness's own emitted text (untrusted DATA: paint it PlainText), `ts` the
    // record's own epoch ms, `clipped` true when the text is a kept prefix.
    // `at` is when the daemon read it — a SNAPSHOT time, not a stream: this is
    // a file read repeated by the caller's own cadence, and nothing here is a
    // token stream.
    //
    // A query is BOUNDED by the daemon whatever this caller asks for (lines
    // clamped, clip validated, the child killed at its wall clock), and it opens
    // its own throwaway socket like sessionAction — one line out, one line back,
    // then closed. The `lines`/`clip` sent here are normalised to what the wire
    // accepts so a widget bug cannot produce a silently unanswered query; the
    // daemon remains the authority.
    function traceSession(sessionId, lines, clip, callback) {
        var n = (typeof lines === "number" && lines > 0) ? Math.min(Math.floor(lines), 40) : 12
        _ask({
            cmd: "sessiontrace",
            sessionId: "" + sessionId,
            lines: n,
            clip: ("" + clip) === "detail" ? "detail" : "line"
        }, callback, { ok: false, reason: "no-client",
                       message: "shellbridge client unavailable" })
    }

    // ── The RICE menu ──────────────────────────────────────────────────────
    // One read and two gestures behind the bar's RICE cell. Any click opens the
    // menu; nothing switches until a row is chosen. None holds a rice rule: the
    // daemon re-execs `lyra rice …`, and the CLI owns the name rules, the
    // minting and every refusal.
    //
    //   bridge.riceMenu(cb)                   // what the menu lists
    //   bridge.riceMode("stage", "fugue")     // stage that runtime song
    //   bridge.riceMode("declarative")        // lock back to the declared song
    //   bridge.riceDraft("enter", "draft-2")  // route the stage into a draft
    //   bridge.riceDraft("new")               // mint the next draft-<n>, enter it
    //   bridge.riceDraft("save")              // snapshot the stage as a draft
    //   bridge.riceDraft("drop", "draft-2")   // delete a saved draft
    //   bridge.riceDeclare("barcarolle", cb)  // the host performs it next rebuild
    //
    // `riceMenu` answers like traceSession: `cb` EXACTLY ONCE, on its own
    // throwaway socket, no replay, the daemon's children bounded
    // together under replyTimeoutMs. The answer is
    // { ok: true, mode, song, draft, stagingSong, declared, songs: [{ name, ok,
    // reason? }], draftsSong, drafts: [{ name, savedAt, current }] } — `song`,
    // `draft`, `stagingSong`, `declared` and `draftsSong` null when unset, and
    // `drafts` empty when no song is named — or { ok: false, reason, message }
    // with `reason` cli-failed / no-answer / no-client. An empty list with
    // ok:true is "none yet"; a refusal is "nobody answered", and the menu must
    // not paint the second as the first.
    //
    // `riceMode` and `riceDraft` are fire-and-forget through sendCommand: no
    // reply, and the outcome is a toast the daemon fires (a command sent while
    // the daemon is down is dropped, so no toast fires). `riceMode` sends a
    // name only for "stage" with a non-empty string, so the daemon drops the
    // line instead of staging a song called "undefined". `riceDraft` `enter`
    // and `new` from the locked declarative mode unlock it first, so a caller
    // re-reads riceMenu to see what happened; `save` and `drop` are refused
    // there (toast: unlock first). `enter` is enter-or-create: the CLI forks the
    // stage into a name that does not exist yet, so a stale row comes back as a
    // copy, not a refusal. `drop` of a missing draft or of the draft in force is
    // the CLI's refusal, toasted. Only `enter` and `drop` take a name.
    //
    // The menu's answer also carries `host`, this machine's name, for the
    // Declare row to speak. `riceDeclare` answers like riceMenu, `cb` EXACTLY
    // ONCE: the daemon runs `lyra rice declare <name> --host`, which rewrites the
    // host config's `habit.song.declared` line and never rebuilds. The answer
    // is { ok: true, message, name, host, changed, stagedDiffers } — `changed`
    // false when the host already declares it, `stagedDiffers` true when the
    // staged copy holds edits the rebuild will not carry, null when unknown —
    // or { ok: false, reason, message } with the CLI's refusal (not-built-in,
    // staged-draft, host-line-count, …) or no-client. A name that is not one
    // argv token is never sent, and the callback answers that itself.
    function riceMenu(callback) {
        _ask({ cmd: "ricemenu" }, callback,
             { ok: false, reason: "no-client", message: "shellbridge client unavailable" })
    }

    function riceMode(action, name) {
        var wanted = { cmd: "ricemode", action: "" + action }
        if (action === "stage" && typeof name === "string" && name.length > 0)
            wanted.name = name
        sendCommand(wanted)
    }

    function riceDeclare(name, callback) {
        if (typeof name !== "string" || name.length === 0 || /\s/.test(name) || name.charAt(0) === "-") {
            if (callback)
                callback({ ok: false, reason: "invalid-name", message: "no song to declare" })
            return
        }
        _ask({ cmd: "ricedeclare", name: name }, callback,
             { ok: false, reason: "no-client", message: "shellbridge client unavailable" })
    }

    function riceDraft(action, name) {
        var wanted = { cmd: "ricedraft", action: "" + action }
        if ((action === "enter" || action === "drop") && typeof name === "string" && name.length > 0)
            wanted.name = name
        sendCommand(wanted)
    }

    // Factory for one-shot action connections — each instance carries exactly
    // one action and self-destructs, the same shape AoideClipboard.qml's
    // previewFactory uses for its one-shot Process.
    property Component actionFactory: Component {
        Socket {
            id: conn
            property var cb: null
            property var wanted: ({})
            property bool done: false

            function finish(reply) {
                if (done)
                    return
                done = true
                deadline.running = false
                connected = false
                if (cb)
                    cb(reply)
                destroy()
            }

            function start() {
                deadline.running = true
                connected = true
            }

            // The guarantee, not the fast path: a peer-close does not reliably
            // flip `connected` (Quickshell 0.3.0), so nothing here waits on a
            // disconnect to decide it heard nothing.
            property Timer deadline: Timer {
                interval: root.replyTimeoutMs
                repeat: false
                onTriggered: conn.finish({ ok: false, message: "no reply from shellbridge" })
            }

            parser: SplitParser {
                splitMarker: "\n"
                onRead: function (line) {
                    var reply = null
                    try {
                        reply = JSON.parse(line)
                    } catch (e) {
                        reply = null
                    }
                    if (!reply || typeof reply !== "object")
                        reply = { ok: false, message: "unreadable reply from shellbridge" }
                    conn.finish(reply)
                }
            }

            onError: function (err) {
                conn.finish({ ok: false, message: "shellbridge unreachable" })
            }

            onConnectedChanged: {
                if (connected) {
                    write(JSON.stringify(wanted) + "\n")
                    flush()
                }
            }
        }
    }

    // ── Generic command sender ─────────────────────────────────────────────
    // Writes one newline-delimited JSON line to the shellbridge socket, over a
    // connection of its own: connect, write, flush, close (see commandFactory
    // below). This is the ONLY outbound path from QML (no hyprctl / MCP / shell
    // exec). sessionAction and the other answered verbs take actionFactory
    // instead, because they wait for a reply on their connection.
    //
    // Nothing is held between commands. On Quickshell 0.3.1 a long-lived Socket
    // that has failed one connect does not reconnect, so one command sent
    // during a daemon outage would silence every later one. A command sent
    // while the daemon is down is dropped with a console warning and is never
    // sent late: each command is a human gesture, and a `power` delivered after
    // the daemon returns would be a shutdown nobody asked for. The connection
    // closes after its write, because the daemon serves one connection per thread.
    function sendCommand(obj) {
        var conn = commandFactory.createObject(root, {
            path: root.socketPath,
            cmd: "" + obj.cmd,
            line: JSON.stringify(obj) + "\n"
        })
        if (conn)
            conn.connected = true
        else
            console.warn("ShellBridge: could not open a socket for '" + obj.cmd + "', dropped")
    }

    // ── Command connection ──────────────────────────────────────────────────
    // A Quickshell.Io.Socket: a real unix-domain client. shellbridge reads
    // newline-delimited JSON and dispatches (e.g. hyprctl focuswindow) — no IPC
    // is invented in QML.
    property Component commandFactory: Component {
        Socket {
            property string cmd: ""
            property string line: ""

            onConnectedChanged: {
                if (connected) {
                    write(line)
                    flush()
                    connected = false
                } else {
                    destroy()
                }
            }

            onError: function (err) {
                console.warn("ShellBridge: dropped '" + cmd + "', shellbridge unreachable")
                destroy()
            }
        }
    }
}
