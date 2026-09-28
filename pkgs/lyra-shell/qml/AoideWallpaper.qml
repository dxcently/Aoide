// AoideWallpaper.qml — wallpaper layer.
//
// Quickshell-native wallpaper rendering — replaces swww/swaybg.
// Reads the active cover path from stage/cover.json (shellbridge emits this
// when a rice adopts or previews a new cover; any writer staging the file
// hot-swaps the wallpaper live via FileView's watcher). Colors from livery
// (solid palette fallback when no cover is staged or the image fails).
//
// stage/cover.json contract (CONTRACTS.md §4):
//   { "path": "/abs/path/to/cover" }              the song's own DEFAULT
//   { "path": "/abs/path/to/cover", "pick": true }  the USER's pick
//
// A song's wallpaper — its cover AND its own live `wallpaper` board — is only
// its DEFAULT. A pick is what shows, full stop: while one stands the board is
// not instantiated at all (below), and it survives re-staging the same song
// (the Rust side's `cover::stage_for_song`).

import QtQuick
import Quickshell
import Quickshell.Io

Item {
    id: root

    required property var livery
    // What the song's `wallpaper` slot anchor below needs (slots.md, "Wired
    // slots"): the staging engine resolves the active song's board, and the
    // bridge is what every widget is handed — a board that reads the machine
    // (sessions/hooks) reads it through the same door as any other surface.
    required property var bridge
    required property var stagingEngine

    // ── Cover path ─────────────────────────────────────────────────────────
    // The BAKED song wallpaper — the lyra lane exports its immutable
    // store path as AOIDE_WALLPAPER, so the song's wallpaper is RELIABLY set on
    // every rebuild/boot (this was the "background gone after rebuild" bug: the
    // live stage/cover.json is runtime state nothing re-seeds from the song).
    readonly property string bakedWallpaper: Quickshell.env("AOIDE_WALLPAPER") || ""

    // The live stage cover (rice preview/adopt or a manual write) OVERRIDES the
    // baked default; falls back to the baked path when the stage is absent/empty.
    property string wallpaperPath: bakedWallpaper

    // Is the staged cover the user's pick rather than the song's own default?
    // One named place to ask, since the board gate below and any later
    // consumer (an external wallpaper engine) both depend on it.
    property bool pickStaged: false

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
                root.pickStaged = !!(d && d.pick === true && d.path)
            } catch (e) {
                root.wallpaperPath = root.bakedWallpaper /* absent/garbage → baked song wallpaper */
                root.pickStaged = false
            }
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

    // ── The song's own board, over that image ──────────────────────────────
    // The `wallpaper` slot (slots.md): the active song may draw its own cover,
    // live, on this surface — cadenza's circuit board moves its light on the
    // copper here.
    //
    // Two gates, and this slot has NO baseline floor: `has` resolves the active
    // song's own manifest entry only, because sonata ships no body for this
    // slot (a baseline twin here painted a second, full-bleed copy of the cover
    // over the image). A pick is what shows, full stop — no hidden repaint
    // under a chosen image, so the gate is a Loader's `active` and the board,
    // with its timer, is destroyed rather than hidden.
    readonly property bool boardActive:
        !root.pickStaged && root.stagingEngine.has(root.livery.songName, "wallpaper")

    Loader {
        id: songBoard
        anchors.fill: parent
        z: 1
        active: root.boardActive
        sourceComponent: boardComponent
    }

    // Full-bleed by definition — the slot anchors to this item rather than
    // reporting an implicit size, so it needs no `extraProps`.
    Component {
        id: boardComponent
        WidgetSlot {
            anchors.fill: parent
            livery: root.livery
            bridge: root.bridge
            stagingEngine: root.stagingEngine
            slot: "wallpaper"
        }
    }
}
