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

    // Is the staged cover the USER's pick rather than the song's own default?
    // The ONE property a reader (or a later lane routing picks through an
    // external engine) needs to ask: `pickStaged` is true only while
    // stage/cover.json carries `"pick": true` AND a path to render. One small
    // named place, so the board gate below never re-derives it.
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
    // The `wallpaper` slot (slots.md, "Wired slots"): the active song may draw
    // its own cover, live, on this surface — cadenza's circuit board moves its
    // light on the copper here.
    //
    // TWO gates, and NOTHING is drawn when either fails:
    //   · `stagingEngine.has(<active song>, "wallpaper")` — the song must
    //     author its OWN `widgets/wallpaper.qml`. This slot has NO baseline
    //     floor (slots.md): sonata ships no body for it, so `has` is exactly
    //     "the active song authors a board", and no other song's body can
    //     ever be resolved into this anchor as a floor. (Before this, sonata's
    //     ported twin body resolved as the floor and painted a second copy of
    //     the cover over this one.)
    //   · `!pickStaged` — a user pick is what shows, full stop. No hidden
    //     repaint under a chosen image; "I'd rather not have it rerender".
    //
    // The gate is a Loader's `active`, so a failing gate DESTROYS the board
    // (and its Timer with it) rather than merely hiding it.
    readonly property bool boardActive:
        !root.pickStaged && root.stagingEngine.has(root.livery.songName, "wallpaper")

    Loader {
        id: songBoard
        anchors.fill: parent
        z: 1
        active: root.boardActive
        sourceComponent: boardComponent
    }

    // The slot is full-bleed by definition — it anchors to this item rather
    // than reporting an implicit size — so it needs no `extraProps`, and the
    // widget reads `livery`/`bridge` like any other. Deliberately NOT a
    // declarative `Loader { source: … }` (WidgetSlot.qml's own header: a
    // `required property` is resolved at OBJECT CREATION, so the properties
    // are handed over through the component's own scope here).
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
