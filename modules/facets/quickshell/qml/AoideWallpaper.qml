// AoideWallpaper.qml — wallpaper layer.
//
// Quickshell-native wallpaper rendering — replaces swww/swaybg.
// Reads the active cover path from stage/cover.json (shellbridge emits this
// when a rice adopts or previews a new cover; any writer staging the file
// hot-swaps the wallpaper live via FileView's watcher). Colors from livery
// (solid palette fallback when no cover is staged or the image fails).
//
// stage/cover.json contract: { "path": "/abs/path/to/cover" }

import QtQuick
import Quickshell
import Quickshell.Io

Item {
    id: root

    required property var livery

    // ── Cover path ─────────────────────────────────────────────────────────
    // The BAKED song wallpaper — the quickshell facet exports its immutable
    // store path as AOIDE_WALLPAPER, so the song's wallpaper is RELIABLY set on
    // every rebuild/boot (this was the "background gone after rebuild" bug: the
    // live stage/cover.json is runtime state nothing re-seeds from the song).
    readonly property string bakedWallpaper: Quickshell.env("AOIDE_WALLPAPER") || ""

    // The live stage cover (rice preview/adopt or a manual write) OVERRIDES the
    // baked default; falls back to the baked path when the stage is absent/empty.
    property string wallpaperPath: bakedWallpaper

    readonly property string coverJsonPath:
        (Quickshell.env("AOIDE_ROOT") || (Quickshell.env("HOME") + "/.aoide")) + "/song/stage/cover.json"

    FileView {
        id: coverFile
        path: root.coverJsonPath
        watchChanges: true
        onTextChanged: {
            try {
                var d = JSON.parse(coverFile.text())
                root.wallpaperPath = (d && d.path) ? ("" + d.path) : root.bakedWallpaper
            } catch (e) { root.wallpaperPath = root.bakedWallpaper /* absent/garbage → baked song wallpaper */ }
        }
        onFileChanged: coverFile.reload()
        Component.onCompleted: coverFile.reload()
    }

    // ── Background: cover image or palette fallback ────────────────────────
    Rectangle {
        anchors.fill: parent
        color: livery.paletteBg  // fallback when no image is set
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
