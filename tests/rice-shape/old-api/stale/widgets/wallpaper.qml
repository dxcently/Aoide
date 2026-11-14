import Quickshell
import QtQuick

Item {
    property string baked: Quickshell.env("AOIDE_WALLPAPER") || ""
}
