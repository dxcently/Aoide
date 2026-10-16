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
// The face receives the universal `livery` + `bridge` and one extra, `auth`,
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
// `yes` from `lockStateChanged` once the compositor grants the lock, `no` from
// the unlock itself, since `WlSessionLock` emits nothing for an unlock it was
// asked for (Quickshell 0.3.1 checks `isLocked()` after releasing the lock).
// The hint marks this host's lock only — hyprlock sets none, and `lyra lock`
// clears a stale `yes` once hyprlock holds the screen. A crashed shell leaves
// the hint at `yes` and the compositor keeps the session locked; this host
// reads the hint when it starts (`lyra lock hint`) and, on `yes`, locks at
// once — Hyprland's `misc:allow_session_lock_restore` hands it the lock back.

import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import Quickshell.Services.Pam

Scope {
    id: root

    required property var livery
    required property var bridge
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

    function lock() {
        if (sessionLock.locked) return
        auth.failures = 0
        root._failed = false
        sessionLock.locked = true
        root._startPam()
    }

    // A conversation that just completed may still read active for a beat,
    // so a restart waits it out rather than leaving the face with nothing
    // to answer.
    function _startPam() {
        if (!sessionLock.locked) return
        if (pam.active) {
            Qt.callLater(root._startPam)
            return
        }
        auth.busy = false
        if (!pam.start())
            root._sync("Authentication could not start.", true)
    }

    function _hint(locked) {
        Quickshell.execDetached(["lyra", "lock", "hint", locked ? "yes" : "no"])
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
            Qt.callLater(root._startPam)
        }
        onError: err => {
            auth.busy = false
            root._sync("Authentication error: " + PamError.toString(err), true)
            Qt.callLater(root._startPam)
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
                    root.lock()
                }
            }
        }
    }

    property IpcHandler _ipc: IpcHandler {
        target: "lock"
        function lock(): void { root.lock() }
        function secure(): bool { return sessionLock.secure }
    }

    WlSessionLock {
        id: sessionLock

        onLockStateChanged: root._hint(sessionLock.locked)

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
                    "bridge": root.bridge,
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
