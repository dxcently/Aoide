// LockHost.qml — the shell's session lock: the session-lock host anchor
// (slots.md, "Three anchor kinds").
//
// One `WlSessionLock` and one `PamContext` for the whole shell; the rice only
// paints. Per screen the lock creates a `WlSessionLockSurface`, and inside it
// the active rice's own `widgets/lockscreen.qml` face, resolved through the
// staging engine like a `SurfaceSlot` but with NO baseline fallback: a rice
// that ships no face is a hyprlock rice (`strillix lock` decides that by the same
// manifest entry and never asks this host to lock).
//
// Entry: `strillix lock` (pkgs/aoide/crates/strillix/src/commands/lock.rs) calls
// `quickshell ipc call lock lock`, then polls `ipc call lock secure` until the
// compositor confirms. Nothing else locks through here.
//
// PAM first, the lock second: `lock` starts the `aoide-lock` conversation and
// only then asks the compositor for the lock. A conversation that cannot
// start (no `/etc/pam.d/aoide-lock`) means no lock at all — `lock` answers
// `false` and `strillix lock` runs hyprlock — never a screen that nothing can
// open. A lock the compositor denies aborts the conversation. After a
// refused password (a plain `Failed`, or `MaxTries`) the conversation
// restarts once the old one reports itself inactive.
//
// A PAM stack that starts but then fails (`onError`: a module that will not
// load, a broken stack) cannot open the lock, so the host hands it over:
// once the lock is secure it runs `strillix lock --hyprlock`, and Hyprland
// (`misc:allow_session_lock_restore`, src/managers/SessionLockManager.cpp
// onNewSessionLock) gives hyprlock the held lock — the session never
// unlocks. The hand-over runs in its own transient scope (`systemd-run
// --user --scope`), so hyprlock lives outside this shell's unit and survives
// it. Only when hyprlock is up does this host let go of its lock, by
// destroying its `WlSessionLock` without unlocking (Quickshell sends
// `destroy`, not `unlock_and_destroy`, which leaves hyprlock's lock
// standing), then quits so the unit restarts a fresh shell: Quickshell 0.3.1
// cannot take a second lock in a process whose lock was destroyed held. A
// conversation that cannot restart while locked is handed over the same way.
// If hyprlock cannot take it either, the host keeps the lock: closed, never
// open.
//
// The face receives the universal `theme`, `bridge` as null — no shellbridge
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
// active rice, a face that fails to compile, one that cannot be created (a
// missing required property), or one that has not set `auth.ready` within
// `faceReadyMs` of its creation (a face whose own start-up threw, and so may
// paint without a field): that face is destroyed. So a locked screen is never
// without input. 3 s: a face is created synchronously and sets `ready` from
// its own `Component.onCompleted`, so a working face is ready in the frame it
// appears; the wait only bounds how long a broken one stands before the floor.
//
// LockedHint: `strillix lock hint yes|no` follows the lock, so the popup
// watchers' locked check sees this lock the way it sees hyprlock's process:
// `yes` once the compositor reports the lock secure, `no` from the unlock
// itself (`WlSessionLock` emits nothing for an unlock it was asked for:
// Quickshell 0.3.1 checks `isLocked()` after releasing the lock) or from a
// lock the compositor dropped. The writes are serialized — one `strillix lock
// hint` at a time, the last state wanted written last, none repeated — so a
// denied lock never leaves a `yes` behind.
// The hint marks this host's lock only — hyprlock sets none, and `strillix lock`
// clears a stale `yes` once hyprlock holds the screen. A crashed shell leaves
// the hint at `yes` and the compositor keeps the session locked; this host
// reads the hint when it starts (`strillix lock hint`) and, on `yes`, locks at
// once — Hyprland's `misc:allow_session_lock_restore` hands it the lock back.
// If its PAM conversation cannot start it hands the lock to `strillix lock`,
// which then runs hyprlock.

import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import Quickshell.Services.Pam

Scope {
    id: root

    required property var theme
    required property var stagingEngine

    readonly property string faceSlot: "lockscreen"
    readonly property int faceReadyMs: 3000
    readonly property string faceSource:
        stagingEngine.has(theme.riceName, faceSlot) ? stagingEngine.source(theme.riceName, faceSlot) : ""

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

    // The current session lock; replaced after a hand-over.
    readonly property var sessionLock: lockLoader.item

    // Starts the conversation, then asks for the lock; false when PAM cannot
    // start, and then nothing is locked. A lock already held gets a live
    // conversation if it has none.
    function lock() {
        if (!sessionLock) return false
        if (sessionLock.locked) {
            if (!pam.active && !root._pamAgain && !root._handingOver) root._restartPam()
            return true
        }
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
        if (!sessionLock || !sessionLock.locked || root._handingOver) return
        auth.busy = false
        if (!pam.start()) root._handOver("the conversation could not restart")
    }

    // ── The hand-over to hyprlock ─────────────────────────────────────────
    property bool _handingOver: false
    property bool _handOverWanted: false

    function _handOver(why) {
        if (root._handingOver) return
        root._handingOver = true
        root._pamAgain = false
        if (pam.active) pam.abort()
        console.warn("[aoide/lockhost] PAM failed while locked (" + why + "); handing the lock to hyprlock")
        root._sync("Authentication is unavailable here; hyprlock is taking over.", true)
        auth.responseRequired = false
        // Hyprland hands a lock over only once the one held is confirmed.
        if (sessionLock.secure) handover.running = true
        else root._handOverWanted = true
    }

    function _handedOver(said) {
        root._handingOver = false
        if (!said || !said.data || said.data.locker !== "hyprlock") {
            console.warn("[aoide/lockhost] hyprlock did not take the lock; this host keeps it")
            root._sync("Authentication is unavailable, and hyprlock did not start.", true)
            return
        }
        console.info("[aoide/lockhost] hyprlock holds the lock; letting go of this one without unlocking")
        // `strillix lock` cleared the hint once hyprlock held the screen.
        root._hintHave = false
        root._hintWant = false
        lockLoader.active = false
        // Quickshell 0.3.1 never clears its process-wide lock manager when a
        // held lock is destroyed (src/wayland/session_lock/lock.cpp:16-21 never
        // resets manager->active, which manager.cpp:15 refuses on), so
        // this process can never lock again: exit, and the unit's
        // Restart=always brings back a shell that can.
        Qt.callLater(Qt.quit)
    }

    property Process _handover: Process {
        id: handover
        // hyprlock must outlive this shell (it quits right after, and a unit
        // stop kills its whole cgroup), so the hand-over runs in its own scope.
        command: ["systemd-run", "--user", "--scope", "--collect", "--quiet", "strillix", "lock", "--hyprlock", "--json"]
        stdout: StdioCollector {
            onStreamFinished: {
                var said = null
                try { said = JSON.parse(text) } catch (e) {}
                root._handedOver(said)
            }
        }
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
        hintWriter.command = ["strillix", "lock", "hint", root._hintHave ? "yes" : "no"]
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
            if (result !== PamResult.Failed && result !== PamResult.MaxTries) {
                root._handOver(PamResult.toString(result))
                return
            }
            auth.failures += 1
            root._sync("The password is incorrect.", true)
            root._failed = true
            root._restartPam()
        }
        onError: err => {
            auth.busy = false
            if (sessionLock && sessionLock.locked) root._handOver(PamError.toString(err))
        }
    }

    property Process _whoami: Process {
        command: ["id", "-un"]
        running: true
        stdout: StdioCollector { onStreamFinished: auth.userName = text.trim() }
    }

    // A lock a crashed predecessor held is taken back at start.
    property Process _relock: Process {
        command: ["strillix", "lock", "hint", "--json"]
        running: true
        stdout: StdioCollector {
            onStreamFinished: {
                var said = null
                try { said = JSON.parse(text) } catch (e) { return }
                if (said && said.data && said.data.lockedHint === true) {
                    console.info("[aoide/lockhost] LockedHint is still yes; taking the lock back")
                    root._hintHave = true
                    root._hintWant = true
                    if (!root.lock()) Quickshell.execDetached(["strillix", "lock"])
                }
            }
        }
    }

    property IpcHandler _ipc: IpcHandler {
        target: "lock"
        function lock(): bool { return root.lock() }
        function secure(): bool { return !!sessionLock && sessionLock.secure }
    }

    LazyLoader {
        id: lockLoader
        active: true

        WlSessionLock {
            id: lockObj

            onSecureStateChanged: {
                if (!lockObj.secure) return
                root._hint(true)
                if (root._handOverWanted) {
                    root._handOverWanted = false
                    handover.running = true
                }
            }
            onLockStateChanged: {
                if (lockObj.locked) return
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
                        "theme": root.theme,
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
}
