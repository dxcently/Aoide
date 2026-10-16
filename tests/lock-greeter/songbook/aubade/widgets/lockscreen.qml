// The fixture lock face: one password field, the host's message and a
// failure count, all through the host's `auth`. It logs a Ctrl+Alt+Del that
// reaches it, so the test can tell whether the compositor lets one through.
import QtQuick

Rectangle {
    id: face
    property var livery
    property var bridge
    property var auth

    color: "#1d3557"

    Column {
        anchors.centerIn: parent
        spacing: 14

        Text {
            text: "AUBADE LOCK FACE · " + face.auth.userName
            color: "#f1faee"
            font.pixelSize: 30
        }

        Rectangle {
            width: 420
            height: 48
            color: "#0b1d33"
            border.color: "#a8dadc"
            border.width: 2

            TextInput {
                id: field
                anchors.fill: parent
                anchors.margins: 12
                color: "#f1faee"
                font.pixelSize: 20
                echoMode: TextInput.Password
                focus: true
                Keys.onPressed: event => {
                    if (event.key === Qt.Key_Delete && (event.modifiers & Qt.ControlModifier) && (event.modifiers & Qt.AltModifier))
                        console.log("aubade: ctrl+alt+delete reached the lock face")
                }
                onAccepted: {
                    face.auth.respond(text)
                    text = ""
                }
            }
        }

        Text {
            text: face.auth.message
            color: face.auth.messageIsError ? "#ff6b6b" : "#a8dadc"
            font.pixelSize: 18
        }

        Text {
            text: "failures: " + face.auth.failures
            color: "#a8dadc"
            font.pixelSize: 18
        }
    }

    Component.onCompleted: field.forceActiveFocus()
}
