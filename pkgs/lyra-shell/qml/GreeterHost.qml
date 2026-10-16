// GreeterHost.qml — the login screen's host: the greeter anchor (slots.md,
// "Three anchor kinds"), and the entry point of the greeter's own shell tree.
//
// The greeter lane (modules/dendrites/greeter.nix) builds a store tree with
// this file as `run/qml/shell.qml`, LiveryState + StagingEngine beside it, the
// DECLARED song's widgets under `songs/<owner>/` and its dress as
// `song/stage/livery.json`, and points `AOIDE_ROOT` at it — nothing here reads
// /home. greetd runs it as its greeter user inside cage.
//
// Authentication is greetd's (`Quickshell.Services.Greetd`): `createSession`,
// `respond`, then `launch` with the session the lane wrote into the tree
// (`session.json`: the Exec of the display manager's default session, and its
// desktop name for `XDG_SESSION_DESKTOP`; `XDG_SESSION_TYPE` is `wayland`, the
// only kind the lane picks from). No session starts without greetd's say-so.
//
// The face — the declared song's `widgets/greeter.qml` — receives `livery`,
// `bridge` (null: a greeter has no shellbridge) and one extra, `auth`:
// `message`, `messageIsError`, `responseRequired`, `responseVisible`, `busy`,
// `failures` (a failure's message stays up until the next answer, under the
// re-prompt), `users` (passwd accounts with uid >= 1000 and a login shell, as
// `{ name, realName }`), `selectedUser`, `selectUser(name)` (starts that
// account's conversation; "" goes back to choosing), and `respond(text)`.
//
// THE FLOOR: as on the lock, this file's own plain user + password fields sit
// under the face and show whenever the face is missing or fails to load.

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

        function selectUser(name) {
            if (Greetd.state !== GreetdState.Inactive) Greetd.cancelSession()
            auth.selectedUser = name
            root._failed = false
            auth.message = ""
            auth.messageIsError = false
            auth.responseRequired = false
            if (name !== "") Qt.callLater(root._begin)
        }

        function respond(text) {
            if (!auth.responseRequired) return
            auth.busy = true
            auth.responseRequired = false
            root._failed = false
            Greetd.respond(text)
        }
    }

    function _begin() {
        if (auth.selectedUser === "" || Greetd.state !== GreetdState.Inactive) return
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
            Qt.callLater(root._begin)
        }
        function onReadyToLaunch() {
            if (root.sessionCommand.length === 0) {
                auth.message = "No session to start."
                auth.messageIsError = true
                Greetd.cancelSession()
                return
            }
            Greetd.launch(root.sessionCommand, ["XDG_SESSION_TYPE=wayland", "XDG_SESSION_DESKTOP=" + root.sessionDesktop], true)
        }
        function onError(error) {
            auth.busy = false
            auth.message = error
            auth.messageIsError = true
            auth.responseRequired = false
            Qt.callLater(root._begin)
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
            var item = comp.createObject(faceHolder, { "livery": livery, "bridge": null, "auth": auth })
            if (!item) {
                console.warn("[aoide/greeter] the face could not be created; the floor stays")
                return
            }
            item.anchors.fill = faceHolder
            win.face = item
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
