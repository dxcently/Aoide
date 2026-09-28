// AoideWallpaper.qml — wallpaper layer.
//
// Quickshell-native wallpaper rendering — replaces swww/swaybg.
// Reads the active cover path from stage/cover.json (shellbridge emits this
// when a rice adopts or previews a new cover; any writer staging the file
// hot-swaps the wallpaper live via FileView's watcher). Colors from livery
// (solid palette fallback when no cover is staged or the image fails).
//
// stage/cover.json contract (CONTRACTS.md §4): the staged cover — `path` for a
// still or a video, `weId` for a scene — with `pick: true` when it is the
// user's own choice and `song` naming the song it was staged for. A cover
// naming another song is ignored.
//
// Who PAINTS that pick is the host's: while an external provider is active and
// a pick applies, this layer paints nothing and the provider's own surface has
// it; otherwise the layer behaves exactly as it always has.

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

    // The live staged cover (rice preview/adopt or a manual write). A cover
    // carries the song it was staged for: one naming another song is ignored,
    // and the layer falls back to the baked default below.
    property string stagedCoverPath: ""
    property string stagedCoverSong: ""
    property string stagedCoverWeId: ""
    property bool stagedCoverPick: false
    readonly property bool stagedCoverApplies:
        root.stagedCoverSong.length === 0 || root.stagedCoverSong === root.livery.songName

    // A pick APPLIES with an identity to show: a path, or a scene's workshop id
    // (CONTRACTS.md §4). A marker with neither is nothing.
    readonly property bool pickApplies:
        root.stagedCoverPick && root.stagedCoverApplies
        && (root.stagedCoverPath.length > 0 || root.stagedCoverWeId.length > 0)

    // An EXTERNAL provider paints that pick on its own surface, so this layer
    // paints nothing at all: no image, no board, no palette rectangle.
    readonly property bool externalProviderShowsPick:
        root.livery.wallpaperProvider === "skwd-wall" && root.pickApplies

    readonly property string wallpaperPath:
        root.stagedCoverApplies && root.stagedCoverPath.length > 0 ? root.stagedCoverPath : root.bakedWallpaper

    readonly property string coverJsonPath:
        (Quickshell.env("AOIDE_ROOT") || (Quickshell.env("HOME") + "/.aoide")) + "/song/stage/cover.json"

    FileView {
        id: coverFile
        path: root.coverJsonPath
        watchChanges: true
        onTextChanged: {
            try {
                var d = JSON.parse(coverFile.text())
                root.stagedCoverPath = (d && d.path) ? ("" + d.path) : ""
                root.stagedCoverSong = (d && d.song) ? ("" + d.song) : ""
                root.stagedCoverWeId = (d && d.weId) ? ("" + d.weId) : ""
                root.stagedCoverPick = !!(d && d.pick === true)
            } catch (e) {
                root.stagedCoverPath = ""
                root.stagedCoverSong = ""
                root.stagedCoverWeId = ""
                root.stagedCoverPick = false
            }
        }
        onFileChanged: coverFile.reload()
        Component.onCompleted: coverFile.reload()
    }

    // ── Background: cover image or palette fallback ────────────────────────
    Rectangle {
        anchors.fill: parent
        color: livery.paletteBg  // fallback when no image is set
        visible: !wallpaperImage.visible && !root.externalProviderShowsPick
    }

    Image {
        id: wallpaperImage
        anchors.fill: parent
        source: root.wallpaperPath.length > 0 ? "file://" + root.wallpaperPath : ""
        fillMode: Image.PreserveAspectCrop
        smooth: true
        asynchronous: true
        visible: status === Image.Ready && !root.externalProviderShowsPick
    }

    // ── The song's own board, over that image ──────────────────────────────
    // The `wallpaper` slot (slots.md): the active song may draw its own cover,
    // live, on this surface — cadenza's circuit board moves its light on the
    // copper here.
    //
    // This slot has NO baseline floor: `has` resolves the active song's own
    // manifest entry, and sonata ships no body for it. Loader, not visible: a
    // pick destroys the board so its timer stops.
    readonly property bool boardActive:
        !root.pickApplies && !root.externalProviderShowsPick
        && root.stagingEngine.has(root.livery.songName, "wallpaper")

    Loader {
        id: songBoard
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
            livery: root.livery
            bridge: root.bridge
            stagingEngine: root.stagingEngine
            slot: "wallpaper"
        }
    }
}
