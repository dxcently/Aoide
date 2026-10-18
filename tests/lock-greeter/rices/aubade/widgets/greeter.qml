// The fixture greeter face: an account field while none is chosen, then one
// password field for the chosen account; Escape goes back to choosing
// (switch user). The host's message and a failure count, all through `auth`.
// An icon from the rice's own icons/, by the path a face uses in the session,
// and a helper type (AubadeCard) from its own folder.
import QtQuick
import Quickshell

Rectangle {
    id: face
    property var theme
    property var bridge
    property var auth

    readonly property bool choosing: face.auth.selectedUser === ""

    color: "#3d1d57"

    onChoosingChanged: {
        field.text = ""
        field.forceActiveFocus()
    }

    Column {
        anchors.centerIn: parent
        spacing: 14

        Image {
            source: "file://" + Quickshell.env("STRILLIX_ROOT") + "/rices/aubade/icons/sun.svg"
            onStatusChanged: if (status === Image.Ready) console.log("[fixture] greeter icon loaded")
        }

        AubadeCard {
            text: "AUBADE GREETER FACE"
        }

        Text {
            text: (face.choosing ? "account? " : "user: " + face.auth.selectedUser) + " of " + face.auth.users.length
            color: "#e0c3fc"
            font.pixelSize: 20
        }

        Rectangle {
            width: 420
            height: 48
            color: "#200b33"
            border.color: "#e0c3fc"
            border.width: 2

            TextInput {
                id: field
                anchors.fill: parent
                anchors.margins: 12
                color: "#f1faee"
                font.pixelSize: 20
                echoMode: face.choosing ? TextInput.Normal : TextInput.Password
                focus: true
                Keys.onEscapePressed: face.auth.selectUser("")
                onAccepted: {
                    if (face.choosing) face.auth.selectUser(text)
                    else face.auth.respond(text)
                    text = ""
                }
            }
        }

        Text {
            text: face.auth.message
            color: face.auth.messageIsError ? "#ff6b6b" : "#e0c3fc"
            font.pixelSize: 18
        }

        Text {
            text: "failures: " + face.auth.failures
            color: "#e0c3fc"
            font.pixelSize: 18
        }
    }

    Component.onCompleted: {
        field.forceActiveFocus()
        face.auth.ready = true
        console.log("[fixture] greeter face ready")
    }
}
