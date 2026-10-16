// LockHost.qml — the shell's session lock: the session-lock host anchor
// (slots.md, "Three anchor kinds").
//
// One `WlSessionLock` and one `PamContext` for the whole shell; the song only
// paints. Per screen the lock creates a `WlSessionLockSurface`, and inside it
// the active song's own `widgets/lockscreen.qml` face, resolved through the
// staging engine like a `SurfaceSlot` but with NO baseline fallback: a song
// that ships no face is a hyprlock song (`lyra lock` decides that by the same
// manifest entry and never asks this host to lock).
//
// Entry: `lyra lock` (pkgs/aoide/crates/lyra/src/commands/lock.rs) calls
// `quickshell ipc call lock lock`, then polls `ipc call lock secure` until the
// compositor confirms. Nothing else locks through here.
//
// PAM first, the lock second: `lock` starts the `aoide-lock` conversation and
// only then asks the compositor for the lock. A conversation that cannot
// start (no `/etc/pam.d/aoide-lock`) means no lock at all — `lock` answers
// `false` and `lyra lock` runs hyprlock — never a screen that nothing can
// open. A lock the compositor denies aborts the conversation. After a
// refused password the conversation restarts once the old one reports
// itself inactive.
//
// The face receives the universal `livery`, `bridge` as null — no shellbridge
// behind a locked screen, so no power or other action reaches the bridge from
// it, the same as the greeter — and one extra, `auth`,
// whose plain properties mirror the PAM conversation: `message`,
// `messageIsError`, `responseRequired`, `responseVisible`, `busy`,
// `failures` (a failure's message stays up until the next answer, under the
// re-prompt), `userName` (the passwd name, from `id -un`), the
// `respond(text)` function and the `unlocked()` signal. A face never sees the
// `PamContext`, and answers it only through `respond`. One property flows the
// other way: the face sets `auth.ready = true` once it is up.
//
// THE FLOOR: every surface carries this file's own plain password field
// under the face. It shows whenever the face is not there — no face in the
// active song, a face that fails to compile, one that cannot be created (a
// missing required property), or one that has not set `auth.ready` within
// `faceReadyMs` of its creation (a face whose own start-up threw, and so may
// paint without a field): that face is destroyed. So a locked screen is never
// without input. 3 s: a face is created synchronously and sets `ready` from
// its own `Component.onCompleted`, so a working face is ready in the frame it
// appears; the wait only bounds how long a broken one stands before the floor.
//
// LockedHint: `lyra lock hint yes|no` follows the lock, so the popup
// watchers' locked check sees this lock the way it sees hyprlock's process:
// `yes` once the compositor reports the lock secure, `no` from the unlock
// itself (`WlSessionLock` emits nothing for an unlock it was asked for:
// Quickshell 0.3.1 checks `isLocked()` after releasing the lock) or from a
// lock the compositor dropped. The writes are serialized — one `lyra lock
// hint` at a time, the last state wanted written last, none repeated — so a
// denied lock never leaves a `yes` behind.
// The hint marks this host's lock only — hyprlock sets none, and `lyra lock`
// clears a stale `yes` once hyprlock holds the screen. A crashed shell leaves
// the hint at `yes` and the compositor keeps the session locked; this host
// reads the hint when it starts (`lyra lock hint`) and, on `yes`, locks at
// once — Hyprland's `misc:allow_session_lock_restore` hands it the lock back.
// If its PAM conversation cannot start it hands the lock to `lyra lock`,
// which then runs hyprlock.

import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import Quickshell.Services.Pam

Scope {
    id: root

    required property var livery
    required property var stagingEngine

    readonly property string faceSlot: "lockscreen"
    readonly property int faceReadyMs: 3000
    readonly property string faceSource:
        stagingEngine.has(livery.songName, faceSlot) ? stagingEngine.source(livery.songName, faceSlot) : ""

    // What a face reads and calls — plain values, never the PamContext.
    readonly property QtObject auth: QtObject {
        id: auth
        property string message: ""
        property bool messageIsError: false
        property bool responseRequired: false
        property bool responseVisible: false
        property bool busy: false
        property int failures: 0
        property string userName: ""
        property bool ready: false
        signal unlocked()

        function respond(text) {
            if (!pam.active || !pam.responseRequired) return
            auth.busy = true
            root._failed = false
            pam.respond(text)
        }
    }

    property bool _failed: false
    property bool _pamAgain: false

    // Starts the conversation, then asks for the lock; false when PAM cannot
    // start, and then nothing is locked.
    function lock() {
        if (sessionLock.locked) return true
        auth.failures = 0
        auth.busy = false
        root._failed = false
        root._pamAgain = false
        if (pam.active) pam.abort()
        if (!pam.start()) {
            console.warn("[aoide/lockhost] the aoide-lock PAM conversation cannot start; not locking")
            return false
        }
        sessionLock.locked = true
        return true
    }

    // A conversation that just ended may still read active; the restart then
    // waits for its `activeChanged`.
    function _restartPam() {
        if (pam.active) {
            root._pamAgain = true
            return
        }
        root._pamAgain = false
        if (!sessionLock.locked) return
        auth.busy = false
        if (!pam.start())
            root._sync("Authentication could not start.", true)
    }

    property bool _hintWant: false
    property bool _hintHave: false

    function _hint(locked) {
        root._hintWant = locked
        root._hintFlush()
    }

    function _hintFlush() {
        if (hintWriter.running || root._hintWant === root._hintHave) return
        root._hintHave = root._hintWant
        hintWriter.command = ["lyra", "lock", "hint", root._hintHave ? "yes" : "no"]
        hintWriter.running = true
    }

    property Process _hintWriter: Process {
        id: hintWriter
        onExited: root._hintFlush()
    }

    function _sync(message, isError) {
        auth.message = message
        auth.messageIsError = isError
        auth.responseRequired = pam.responseRequired
        auth.responseVisible = pam.responseVisible
    }

    property PamContext _pam: PamContext {
        id: pam
        config: "aoide-lock"

        onActiveChanged: if (!pam.active && root._pamAgain) root._restartPam()

        onPamMessage: {
            auth.busy = false
            if (root._failed) {
                auth.responseRequired = pam.responseRequired
                auth.responseVisible = pam.responseVisible
            } else {
                root._sync(pam.message, pam.messageIsError)
            }
        }
        onCompleted: result => {
            auth.busy = false
            if (result === PamResult.Success) {
                auth.message = ""
                auth.responseRequired = false
                sessionLock.locked = false
                root._hint(false)
                auth.unlocked()
                return
            }
            auth.failures += 1
            root._sync("The password is incorrect.", true)
            root._failed = true
            root._restartPam()
        }
        onError: err => {
            auth.busy = false
            root._sync("Authentication error: " + PamError.toString(err), true)
            root._restartPam()
        }
    }

    property Process _whoami: Process {
        command: ["id", "-un"]
        running: true
        stdout: StdioCollector { onStreamFinished: auth.userName = text.trim() }
    }

    // A lock a crashed predecessor held is taken back at start.
    property Process _relock: Process {
        command: ["lyra", "lock", "hint", "--json"]
        running: true
        stdout: StdioCollector {
            onStreamFinished: {
                var said = null
                try { said = JSON.parse(text) } catch (e) { return }
                if (said && said.data && said.data.lockedHint === true) {
                    console.info("[aoide/lockhost] LockedHint is still yes; taking the lock back")
                    root._hintHave = true
                    root._hintWant = true
                    if (!root.lock()) Quickshell.execDetached(["lyra", "lock"])
                }
            }
        }
    }

    property IpcHandler _ipc: IpcHandler {
        target: "lock"
        function lock(): bool { return root.lock() }
        function secure(): bool { return sessionLock.secure }
    }

    WlSessionLock {
        id: sessionLock

        onSecureStateChanged: if (sessionLock.secure) root._hint(true)
        onLockStateChanged: {
            if (sessionLock.locked) return
            root._pamAgain = false
            if (pam.active) pam.abort()
            root._hint(false)
        }

        WlSessionLockSurface {
            id: surface
            color: "#000000"

            property var face: null

            Component.onCompleted: surface._build()

            function _build() {
                if (root.faceSource === "") return
                var comp = Qt.createComponent(root.faceSource)
                if (comp.status === Component.Loading)
                    comp.statusChanged.connect(function () { surface._finish(comp) })
                else
                    surface._finish(comp)
            }

            function _finish(comp) {
                if (comp.status !== Component.Ready) {
                    if (comp.status === Component.Error)
                        console.warn("[aoide/lockhost] the face failed to load:", comp.errorString())
                    return
                }
                root.auth.ready = false
                var item = comp.createObject(faceHolder, {
                    "livery": root.livery,
                    "bridge": null,
                    "auth": root.auth
                })
                if (!item) {
                    console.warn("[aoide/lockhost] the face could not be created; the floor stays")
                    return
                }
                item.anchors.fill = faceHolder
                surface.face = item
                readyWatch.restart()
            }

            Timer {
                id: readyWatch
                interval: root.faceReadyMs
                onTriggered: {
                    if (surface.face === null || root.auth.ready) return
                    console.warn("[aoide/lockhost] the face did not report ready within", interval, "ms; the floor takes over")
                    surface.face.destroy()
                    surface.face = null
                }
            }

            // The floor: a plain password field, under the face, shown
            // whenever there is no face.
            Rectangle {
                id: floor
                anchors.fill: parent
                color: "#101014"
                visible: surface.face === null

                Column {
                    anchors.centerIn: parent
                    spacing: 14
                    width: 360

                    Text {
                        width: parent.width
                        horizontalAlignment: Text.AlignHCenter
                        text: root.auth.userName
                        color: "#e8e8ec"
                        font.pixelSize: 22
                    }

                    Rectangle {
                        width: parent.width
                        height: 44
                        radius: 6
                        color: "#1e1e24"
                        border.color: floorField.activeFocus ? "#8ab4f8" : "#4a4a55"
                        border.width: 2

                        TextInput {
                            id: floorField
                            objectName: "lockFloorField"
                            anchors.fill: parent
                            anchors.margins: 12
                            verticalAlignment: TextInput.AlignVCenter
                            color: "#e8e8ec"
                            font.pixelSize: 18
                            echoMode: TextInput.Password
                            enabled: floor.visible && !root.auth.busy
                            focus: floor.visible
                            onAccepted: {
                                root.auth.respond(text)
                                text = ""
                            }
                        }
                    }

                    Text {
                        width: parent.width
                        horizontalAlignment: Text.AlignHCenter
                        wrapMode: Text.Wrap
                        text: root.auth.message
                        color: root.auth.messageIsError ? "#ff8a80" : "#b0b0b8"
                        font.pixelSize: 14
                    }
                }

                onVisibleChanged: if (visible) floorField.forceActiveFocus()
                Component.onCompleted: if (visible) floorField.forceActiveFocus()
            }

            Item {
                id: faceHolder
                anchors.fill: parent
            }
        }
    }
}
