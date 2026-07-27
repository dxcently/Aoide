// AoideWallpaper.qml — wallpaper layer.
//
// Quickshell-native wallpaper rendering — replaces swww/swaybg.
// Reads the active cover path from stage/cover.json (shellbridge emits this
// when a rice adopts or previews a new cover; any writer staging the file
// hot-swaps the wallpaper live via FileView's watcher). Colors from notes
// (solid palette fallback when no cover is staged or the image fails).
//
// stage/cover.json contract: { "path": "/abs/path/to/cover" }

import QtQuick
import Quickshell
import Quickshell.Io

Item {
    id: root

    required property var notes

    // ── Cover path from stage (hot-reloading) ──────────────────────────────
    property string wallpaperPath: ""

    readonly property string coverJsonPath:
        Quickshell.env("HOME") + "/Aoide/song/stage/cover.json"

    FileView {
        id: coverFile
        path: root.coverJsonPath
        watchChanges: true
        onTextChanged: {
            try {
                var d = JSON.parse(coverFile.text())
                root.wallpaperPath = (d && d.path) ? ("" + d.path) : ""
            } catch (e) { /* absent/garbage → keep current (fallback shows) */ }
        }
        onFileChanged: coverFile.reload()
        Component.onCompleted: coverFile.reload()
    }

    // ── Background: cover image or palette fallback ────────────────────────
    Rectangle {
        anchors.fill: parent
        color: notes.paletteBg  // fallback when no image is set
        visible: !wallpaperImage.visible
    }

    Image {
        id: wallpaperImage
        anchors.fill: parent
        source: root.wallpaperPath.length > 0 ? "file://" + root.wallpaperPath : ""
        fillMode: Image.PreserveAspectCrop
        smooth: true
        asynchronous: true
        visible: status === Image.Ready
    }
}
