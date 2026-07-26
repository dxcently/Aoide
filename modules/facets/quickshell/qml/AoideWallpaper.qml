// AoideWallpaper.qml — wallpaper layer (skeleton).
//
// Quickshell-native wallpaper rendering — replaces swww/swaybg.
// Reads the active wallpaper path from stage/cover.json (shellbridge emits
// this when a rice adopts or previews a new cover). Colors from notes
// (used for solid-color fallback when no cover is set).

import QtQuick

Item {
    id: root

    required property var notes

    // ── Wallpaper path from stage ──────────────────────────────────────────
    // STUB: bind to stage/cover.json { path: "..." }. For now, solid fallback.
    property string wallpaperPath: ""

    // ── Background: image or palette fallback ─────────────────────────────
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
        visible: status === Image.Ready
    }
}
