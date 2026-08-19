// wallpaper.qml — sonata's "wallpaper" slot: the background layer.
//
// Ported from the facet's AoideWallpaper.qml (per-song widget-slot expansion,
// CONTRACTS.md §5) — ownership moves from the facet to sonata's score; the
// facet original stays in place until a later phase (Phase 6) retires it, so
// this file and AoideWallpaper.qml are momentarily twins. This slot is hosted
// by `WidgetSlot`, not `SurfaceSlot` — the root below is an `Item`, not a
// `PanelWindow`; one instance lives inside the existing wallpaper PanelWindow
// per screen (the facet's own Variants delegate keeps owning the layer,
// namespace and click-through mask — that's whole-content-slot chrome, same
// as the bar's PanelWindow). That WidgetSlot host wiring itself lands with
// Phase 6, not here.
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
    // ShellBridge — every WidgetSlot anchor injects this unconditionally
    // (the universal widget contract, slots.md); unused by this body today.
    required property var bridge

    // WidgetSlot (the host) sizes ITSELF off this item's implicitWidth/
    // Height (WidgetSlot.qml) — but never anchors the loaded child, so
    // nothing gives this Item a size on its own (bar.qml carries the same
    // gotcha, see its own comment). A full-screen background has no natural
    // implicit footprint the way the bar's 36px strip does, so bind both
    // dimensions to the QML `parent` directly instead — under
    // `Component.createObject(root, props)` (WidgetSlot's mechanism), this
    // item's `parent` IS the WidgetSlot instance, and the host anchors THAT
    // to fill the wallpaper PanelWindow (the bar.qml idiom, applied to both
    // axes since there's no fixed strip height here to fall back on).
    width: parent ? parent.width : implicitWidth
    height: parent ? parent.height : implicitHeight

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
        Quickshell.env("HOME") + "/Aoide/song/stage/cover.json"

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
