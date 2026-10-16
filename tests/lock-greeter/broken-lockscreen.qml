// A lock face that fails to load: it names a type that does not exist, so
// the host's component comes back in error and the floor must stay.
import QtQuick

Rectangle {
    property var livery
    property var bridge
    property var auth

    color: "#ff00ff"

    ThisTypeDoesNotExist {}
}
