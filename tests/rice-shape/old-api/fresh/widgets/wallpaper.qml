import Quickshell
import QtQuick

Item {
    property string baked: Quickshell.env("STRILLIX_WALLPAPER") || ""
}
