// StagingEngine.qml — the staging engine: resolves the active song's drachma
// tokens to per-slot QML.
//
// The quickshell facet's build (modules/facets/quickshell/default.nix) carries
// every committed song's authored widget slots into $out/qml/songs/<name>/
// <slot>.qml, plus a generated songs/manifest.json recording which songs
// authored which slots — e.g. `{ "default": ["calendar"], "sonata": ["calendar"] }`.
// This singleton reads that manifest (hot-reloaded, so a rebuild's new
// manifest is picked up without restarting Quickshell) and answers the two
// questions a WidgetSlot needs: does <song> dress <slot> (`has`), and where
// is its QML (`source`).
//
// Fixed injected-prop contract (CONTRACTS.md §5 containment): a loaded song
// widget receives ONLY `notes` (DrachmaState) and `bridge` (ShellBridge) —
// plus whatever slot-specific extras the anchor declares (e.g. notifications'
// `notification`) — never nix `config.*`. A song widget is store-copied
// score, structurally incapable of reaching host/facet options through this
// surface.

import QtQuick
import Quickshell
import Quickshell.Io

QtObject {
    id: root

    readonly property string manifestPath:
        Quickshell.env("HOME") + "/Aoide/run/qml/songs/manifest.json"

    // { "<song>": ["<slot>", …], … } — empty until the first successful parse.
    property var manifest: ({})

    // Declared as a property (not a default-child) because QtObject has no
    // default property — the DrachmaState.noteFile idiom.
    property FileView manifestFile: FileView {
        id: manifestFile
        path: root.manifestPath
        watchChanges: true
        onFileChanged: manifestFile.reload()
        onTextChanged: {
            try {
                root.manifest = JSON.parse(manifestFile.text())
            } catch (e) {
                console.warn("[aoide/stagingengine] Failed to parse songs/manifest.json:", e)
            }
        }
        Component.onCompleted: manifestFile.reload()
    }

    // Does <song> authored a QML file for <slot>? Bounds-checked: an unknown
    // song or a song with no manifest entry both cleanly answer false.
    function has(song, slot) {
        if (!song || !manifest || !manifest[song]) return false
        return manifest[song].indexOf(slot) !== -1
    }

    // Resolved URL for <song>'s <slot> widget — only meaningful when has()
    // is true; callers gate on that first.
    function source(song, slot) {
        return Qt.resolvedUrl("songs/" + song + "/" + slot + ".qml")
    }
}
