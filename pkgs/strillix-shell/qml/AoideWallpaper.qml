// AoideWallpaper.qml — wallpaper layer.
//
// Quickshell-native wallpaper rendering — replaces swww/swaybg.
// Reads the active wallpaper path from stage/wallpaper.json (shellbridge emits this
// when a rice adopts or previews a new wallpaper; any writer staging the file
// hot-swaps the wallpaper live via FileView's watcher). Colors from theme
// (solid palette fallback when no wallpaper is staged or the image fails).
//
// stage/wallpaper.json contract (CONTRACTS.md §4): the staged wallpaper — `path` for a
// still or a video, `weId` for a scene — with `pick: true` when it is the
// user's own choice and `rice` naming the rice it was staged for. A wallpaper
// naming another rice is ignored. A pick with `tile: true` repeats its image
// at 1:1 instead of cropping it to fill.
//
// Who PAINTS that pick is the host's: while an external provider is active and
// a pick applies, this layer paints nothing and the provider's own surface has
// it; otherwise the layer behaves exactly as it always has.

import QtQuick
import Quickshell
import Quickshell.Io

Item {
    id: root

    required property var theme
    // What the rice's `wallpaper` slot anchor below needs (slots.md, "Wired
    // slots"): the staging engine resolves the active rice's board, and the
    // bridge is what every widget is handed — a board that reads the machine
    // (sessions/hooks) reads it through the same door as any other surface.
    required property var bridge
    required property var stagingEngine

    // ── Wallpaper path ─────────────────────────────────────────────────────────
    // The BAKED rice wallpaper — the strillix lane exports its immutable
    // store path as AOIDE_WALLPAPER, so the rice's wallpaper is RELIABLY set on
    // every rebuild/boot (this was the "background gone after rebuild" bug: the
    // live stage/wallpaper.json is runtime state nothing re-seeds from the rice).
    readonly property string bakedWallpaper: Quickshell.env("AOIDE_WALLPAPER") || ""

    // The live staged wallpaper (rice preview/adopt or a manual write). A wallpaper
    // carries the rice it was staged for: one naming another rice is ignored,
    // and the layer falls back to the baked default below.
    property string stagedWallpaperPath: ""
    property string stagedWallpaperRice: ""
    property string stagedWallpaperWeId: ""
    property bool stagedWallpaperPick: false
    property bool stagedWallpaperTile: false
    readonly property bool stagedWallpaperApplies:
        root.stagedWallpaperRice.length === 0 || root.stagedWallpaperRice === root.theme.riceName

    // A pick APPLIES with an identity to show: a path, or a scene's workshop id
    // (CONTRACTS.md §4). A marker with neither is nothing.
    readonly property bool pickApplies:
        root.stagedWallpaperPick && root.stagedWallpaperApplies
        && (root.stagedWallpaperPath.length > 0 || root.stagedWallpaperWeId.length > 0)

    // An EXTERNAL provider paints that pick on its own surface, so this layer
    // paints nothing at all: no image, no board, no palette rectangle.
    readonly property bool externalProviderShowsPick:
        root.theme.wallpaperProvider === "skwd-wall" && root.pickApplies

    // A tile is the pick's own: it lays out the picked path and nothing the
    // layer falls back to.
    readonly property bool pickTiles:
        root.pickApplies && root.stagedWallpaperTile && root.stagedWallpaperPath.length > 0

    readonly property string wallpaperPath:
        root.stagedWallpaperApplies && root.stagedWallpaperPath.length > 0 ? root.stagedWallpaperPath : root.bakedWallpaper

    readonly property string wallpaperJsonPath:
        (Quickshell.env("STRILLIX_ROOT") || (Quickshell.env("HOME") + "/.strillix")) + "/stage/wallpaper.json"

    FileView {
        id: wallpaperFile
        path: root.wallpaperJsonPath
        watchChanges: true
        onTextChanged: {
            try {
                var d = JSON.parse(wallpaperFile.text())
                root.stagedWallpaperPath = (d && d.path) ? ("" + d.path) : ""
                root.stagedWallpaperRice = (d && d.rice) ? ("" + d.rice) : ""
                root.stagedWallpaperWeId = (d && d.weId) ? ("" + d.weId) : ""
                root.stagedWallpaperPick = !!(d && d.pick === true)
                root.stagedWallpaperTile = !!(d && d.tile === true)
            } catch (e) {
                root.stagedWallpaperPath = ""
                root.stagedWallpaperRice = ""
                root.stagedWallpaperWeId = ""
                root.stagedWallpaperPick = false
                root.stagedWallpaperTile = false
            }
        }
        onFileChanged: wallpaperFile.reload()
        Component.onCompleted: wallpaperFile.reload()
    }

    // ── Background: wallpaper image or palette fallback ────────────────────────
    Rectangle {
        anchors.fill: parent
        color: theme.paletteBg  // fallback when no image is set
        visible: !wallpaperImage.visible && !root.externalProviderShowsPick
    }

    Image {
        id: wallpaperImage
        anchors.fill: parent
        source: root.wallpaperPath.length > 0 ? "file://" + root.wallpaperPath : ""
        fillMode: root.pickTiles ? Image.Tile : Image.PreserveAspectCrop
        smooth: true
        asynchronous: true
        visible: status === Image.Ready && !root.externalProviderShowsPick
    }

    // ── The rice's own board, over that image ──────────────────────────────
    // The `wallpaper` slot (slots.md): the active rice may draw its own wallpaper,
    // live, on this surface — cadenza's circuit board moves its light on the
    // copper here.
    //
    // This slot has NO baseline floor: `has` resolves the active rice's own
    // manifest entry, and sonata ships no body for it. Loader, not visible: a
    // pick destroys the board so its timer stops.
    readonly property bool boardActive:
        !root.pickApplies && !root.externalProviderShowsPick
        && root.stagingEngine.has(root.theme.riceName, "wallpaper")

    Loader {
        id: riceBoard
        anchors.fill: parent
        z: 1
        active: root.boardActive
        sourceComponent: boardComponent
    }

    // The slot is full-bleed: it anchors to this item, so it reports no size.
    Component {
        id: boardComponent
        WidgetSlot {
            anchors.fill: parent
            theme: root.theme
            bridge: root.bridge
            stagingEngine: root.stagingEngine
            slot: "wallpaper"
        }
    }
}
