// GreeterHost.qml — the login screen's host: the greeter anchor (slots.md,
// "Three anchor kinds"), and the entry point of the greeter's own shell tree.
//
// The greeter lane (modules/dendrites/greeter.nix) builds a store tree with
// this file as `run/qml/shell.qml`, LiveryState + StagingEngine beside it, the
// DECLARED song's widgets under `songs/<owner>/` (with a `qmldir` naming its
// uppercase helpers as types, as lyra's tree does), its `icons/` (when it has
// one) at `song/songbook/<owner>/icons` — where a face finds them under
// `AOIDE_ROOT` in the session too — and its dress as `song/stage/livery.json`,
// and points `AOIDE_ROOT` at it — nothing here reads /home. greetd runs it as its greeter user inside cage.
//
// Authentication is greetd's (`Quickshell.Services.Greetd`): `createSession`,
// `respond`, then `launch` with the session the lane wrote into the tree
// (`session.json`: the display manager's session wrapper and the default
// session's whole Exec line for it to evaluate, and the desktop name for
// `XDG_SESSION_DESKTOP`; `XDG_SESSION_TYPE` is `wayland`, the only kind the
// lane picks from). No session starts without greetd's say-so, and none
// from a readiness no answer earned. After a cancel the next conversation
// waits out greetd's reply to it (`settle`).
//
// The face — the declared song's `widgets/greeter.qml` — receives `livery`,
// `bridge` (null: a greeter has no shellbridge) and one extra, `auth`:
// `message`, `messageIsError`, `responseRequired`, `responseVisible`, `busy`,
// `failures` (a failure's message stays up until the next answer, under the
// re-prompt), `users` (passwd accounts with uid >= 1000 and a login shell, as
// `{ name, realName }`), `selectedUser`, `selectUser(name)` (starts that
// account's conversation; "" goes back to choosing), and `respond(text)`;
// the face sets `auth.ready = true` once it is up.
//
// THE FLOOR: as on the lock, this file's own plain user + password fields sit
// under the face and show whenever the face is missing, fails to load, or has
// not set `auth.ready` within `faceReadyMs` (3 s, as on the lock) of its
// creation — that face is destroyed.

import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Services.Greetd

ShellRoot {
    id: root

    readonly property string aoideRoot: Quickshell.env("AOIDE_ROOT") || ""

    LiveryState { id: livery }
    StagingEngine { id: stagingEngine }

    readonly property string faceSource:
        stagingEngine.has(livery.songName, "greeter") ? stagingEngine.source(livery.songName, "greeter") : ""

    readonly property int faceReadyMs: 3000
    property var sessionCommand: []
    property string sessionDesktop: ""
    property bool _failed: false

    readonly property QtObject auth: QtObject {
        id: auth
        property string message: ""
        property bool messageIsError: false
        property bool responseRequired: false
        property bool responseVisible: false
        property bool busy: false
        property int failures: 0
        property var users: []
        property string selectedUser: ""
        property bool ready: false

        function selectUser(name) {
            if (Greetd.state !== GreetdState.Inactive) {
                Greetd.cancelSession()
                settle.restart()
            }
            auth.selectedUser = name
            root._failed = false
            auth.message = ""
            auth.messageIsError = false
            auth.responseRequired = false
            if (name === "") settle.stop()
            else if (!settle.running) root._begin()
        }

        function respond(text) {
            if (!auth.responseRequired) return
            auth.busy = true
            auth.responseRequired = false
            root._failed = false
            root._answered = true
            Greetd.respond(text)
        }
    }

    // A cancelled conversation leaves greetd's reply to the cancel still
    // unread, and Quickshell (0.3.1) reads a `success` as the readiness of
    // whatever conversation is current by then. Quickshell flips to Inactive
    // the moment it sends the cancel — before greetd answers — so a session
    // created on that `stateChanged` takes the stray `success` as its own.
    // After any cancel (switch user, and the one Quickshell sends itself
    // after a refused password) the next conversation therefore starts
    // `settle` later, once the reply has been read. The settle is UX only: the
    // real protection is greetd's own refusal to start an unauthenticated
    // session (greetd 0.10.3, greetd/src/context.rs:295 answers start_session
    // "session is not ready" unless the worker reported Success, which
    // greetd/src/session/worker.rs:129-137 sends only after pam.authenticate
    // and acct_mgmt pass).
    Timer {
        id: settle
        interval: 300
        onTriggered: root._begin()
    }

    // Set once this conversation has been answered: greetd never reports an
    // unanswered one ready, so such a readiness is a stray and launches
    // nothing.
    property bool _answered: false
    // A refusal's cancel is still to be answered by greetd.
    property bool _cancelReply: false

    function _begin() {
        root._cancelReply = false
        if (auth.selectedUser === "" || Greetd.state !== GreetdState.Inactive) return
        root._answered = false
        auth.busy = false
        Greetd.createSession(auth.selectedUser)
    }

    Connections {
        target: Greetd
        function onAuthMessage(message, error, responseRequired, echoResponse) {
            auth.busy = false
            if (!root._failed) {
                auth.message = message
                auth.messageIsError = error
            }
            auth.responseRequired = responseRequired
            auth.responseVisible = echoResponse
        }
        function onAuthFailure(message) {
            auth.busy = false
            auth.failures += 1
            root._failed = true
            auth.message = "The user name or password is incorrect."
            auth.messageIsError = true
            auth.responseRequired = false
            root._cancelReply = true
            settle.restart()
        }
        function onReadyToLaunch() {
            if (!root._answered) {
                console.warn("[aoide/greeter] greetd reported an unanswered conversation ready; starting over")
                Greetd.cancelSession()
                settle.restart()
                return
            }
            if (root.sessionCommand.length === 0) {
                auth.message = "No session to start."
                auth.messageIsError = true
                Greetd.cancelSession()
                return
            }
            Greetd.launch(root.sessionCommand, ["XDG_SESSION_TYPE=wayland", "XDG_SESSION_DESKTOP=" + root.sessionDesktop], true)
        }
        function onError(error) {
            // greetd answers the cancel Quickshell sends after a refused
            // password with an error of its own; the refusal stays the word.
            if (root._cancelReply) {
                root._cancelReply = false
                console.info("[aoide/greeter] greetd's reply to the cancel after a refusal:", error)
                return
            }
            auth.busy = false
            auth.message = error
            auth.messageIsError = true
            auth.responseRequired = false
            settle.restart()
        }
    }

    // passwd → the accounts a person logs into.
    FileView {
        path: "/etc/passwd"
        onLoaded: {
            var out = []
            var lines = text().split("\n")
            for (var i = 0; i < lines.length; i++) {
                var f = lines[i].split(":")
                if (f.length < 7) continue
                var uid = parseInt(f[2])
                var shell = f[6]
                if (uid < 1000 || uid >= 60000) continue
                if (shell === "" || /(nologin|false)$/.test(shell)) continue
                out.push({ name: f[0], realName: (f[4] || "").split(",")[0] || f[0] })
            }
            auth.users = out
            if (out.length === 1 && auth.selectedUser === "") auth.selectUser(out[0].name)
        }
    }

    FileView {
        path: root.aoideRoot + "/session.json"
        onLoaded: {
            try {
                var s = JSON.parse(text())
                root.sessionCommand = s.command || []
                root.sessionDesktop = s.desktop || ""
            }
            catch (e) { console.warn("[aoide/greeter] session.json:", e) }
        }
    }

    FloatingWindow {
        id: win
        visible: true
        color: "#000000"

        property var face: null

        Component.onCompleted: win._build()
        Connections {
            target: root
            function onFaceSourceChanged() { if (!win.face) win._build() }
        }

        function _build() {
            if (root.faceSource === "") return
            var comp = Qt.createComponent(root.faceSource)
            if (comp.status === Component.Loading)
                comp.statusChanged.connect(function () { win._finish(comp) })
            else
                win._finish(comp)
        }

        function _finish(comp) {
            if (comp.status !== Component.Ready) {
                if (comp.status === Component.Error)
                    console.warn("[aoide/greeter] the face failed to load:", comp.errorString())
                return
            }
            auth.ready = false
            var item = comp.createObject(faceHolder, { "livery": livery, "bridge": null, "auth": auth })
            if (!item) {
                console.warn("[aoide/greeter] the face could not be created; the floor stays")
                return
            }
            item.anchors.fill = faceHolder
            win.face = item
            readyWatch.restart()
        }

        Timer {
            id: readyWatch
            interval: root.faceReadyMs
            onTriggered: {
                if (win.face === null || auth.ready) return
                console.warn("[aoide/greeter] the face did not report ready within", interval, "ms; the floor takes over")
                win.face.destroy()
                win.face = null
            }
        }

        Rectangle {
            id: floor
            anchors.fill: parent
            color: "#101014"
            visible: win.face === null

            Column {
                anchors.centerIn: parent
                spacing: 14
                width: 360

                Repeater {
                    model: [
                        { field: "user", hint: "user" },
                        { field: "password", hint: "password" }
                    ]
                    Rectangle {
                        required property var modelData
                        width: 360
                        height: 44
                        radius: 6
                        color: "#1e1e24"
                        border.color: input.activeFocus ? "#8ab4f8" : "#4a4a55"
                        border.width: 2

                        TextInput {
                            id: input
                            objectName: "greeterFloor-" + modelData.field
                            anchors.fill: parent
                            anchors.margins: 12
                            verticalAlignment: TextInput.AlignVCenter
                            color: "#e8e8ec"
                            font.pixelSize: 18
                            echoMode: modelData.field === "password" ? TextInput.Password : TextInput.Normal
                            text: modelData.field === "user" ? auth.selectedUser : ""
                            focus: floor.visible && (modelData.field === "password") === (auth.selectedUser !== "")
                            onAccepted: {
                                if (modelData.field === "user") auth.selectUser(text)
                                else { auth.respond(text); text = "" }
                            }
                        }
                    }
                }

                Text {
                    width: 360
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.Wrap
                    text: auth.message
                    color: auth.messageIsError ? "#ff8a80" : "#b0b0b8"
                    font.pixelSize: 14
                }
            }
        }

        Item {
            id: faceHolder
            anchors.fill: parent
        }
    }
}
