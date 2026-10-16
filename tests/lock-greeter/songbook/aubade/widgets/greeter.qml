// The fixture greeter face: the chosen account, one password field, the
// host's message and a failure count, all through the host's `auth`.
import QtQuick

Rectangle {
    id: face
    property var livery
    property var bridge
    property var auth

    color: "#3d1d57"

    Column {
        anchors.centerIn: parent
        spacing: 14

        Text {
            text: "AUBADE GREETER FACE"
            color: "#f1faee"
            font.pixelSize: 30
        }

        Text {
            text: "user: " + (face.auth.selectedUser || "-") + " of " + face.auth.users.length
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
                echoMode: TextInput.Password
                focus: true
                onAccepted: {
                    face.auth.respond(text)
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
    }
}
