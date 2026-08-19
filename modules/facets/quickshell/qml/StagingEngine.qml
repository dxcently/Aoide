// StagingEngine.qml — the staging engine: resolves the active song's livery
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
// widget receives ONLY `livery` (LiveryState) and `bridge` (ShellBridge) —
// plus whatever slot-specific extras the anchor declares (e.g. notifications'
// `notification`) — never nix `config.*`. A song widget is store-copied
// score, structurally incapable of reaching host/facet options through this
// surface.
//
// ── Declared widget-type registry (Phase 4) ─────────────────────────────────
// A second, independent data source: the same build (Phase 2) and
// `rice stage`/`preview` hot-sync (Phase 3) also carry each song's
// `aoide.arrangement.widgets` declarations into a sibling
// songs/registry.json, shaped `{ "<song>": { "<slot>": {…declaration…} } }`
// — which slots a song registers as widget-TYPE declarations (kind/
// namespace/layer/shortcut/blur), a rarer, smaller set than manifest.json's
// "which slot bodies exist". `declaredWidgets(song)` below answers "what did
// <song> register" the way `has`/`source` answer "does <song> dress <slot>"
// — a parallel accessor, not a replacement; `resolveSong`/body-loading are
// unchanged.

import QtQuick
import Quickshell
import Quickshell.Io

QtObject {
    id: root

    readonly property string manifestPath:
        Quickshell.env("HOME") + "/Aoide/run/qml/songs/manifest.json"

    // The baseline-fallback floor (CONTRACTS.md §5): when the ACTIVE song
    // doesn't dress a slot, resolution falls back to this song before
    // falling back further to the anchor's own facet-side `fallback`
    // Component. Mirrors `aoide.song`'s own default
    // (`modules/nucleus/options.nix`) — sonata is the shipped, guaranteed-
    // present baseline, so it's the correct floor to catch every other song's
    // gaps.
    readonly property string baselineSong: "sonata"

    // { "<song>": ["<slot>", …], … } — empty until the first successful parse.
    property var manifest: ({})

    // Declared as a property (not a default-child) because QtObject has no
    // default property — the LiveryState.liveryFile idiom.
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

    readonly property string registryPath:
        Quickshell.env("HOME") + "/Aoide/run/qml/songs/registry.json"

    // { "<song>": { "<slot>": {…declaration…} }, … } — empty until the first
    // successful parse.
    property var registry: ({})

    // Same named-property idiom as manifestFile above (QtObject has no
    // default property) — an independent FileView watching the sibling
    // registry.json, same directory/reload/parse shape as manifestFile.
    property FileView registryFile: FileView {
        id: registryFile
        path: root.registryPath
        watchChanges: true
        onFileChanged: registryFile.reload()
        onTextChanged: {
            try {
                root.registry = JSON.parse(registryFile.text())
            } catch (e) {
                console.warn("[aoide/stagingengine] Failed to parse songs/registry.json:", e)
            }
        }
        Component.onCompleted: registryFile.reload()
    }

    // <song>'s declared widget-type registrations — `{}` when the song has
    // none, or the file hasn't loaded yet. Never throws: an unknown song or
    // a not-yet-parsed registry both cleanly answer `{}`, same
    // bounds-checked posture as `has` below.
    function declaredWidgets(song) {
        if (!song || !root.registry || !root.registry[song]) return {}
        return root.registry[song]
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

    // ── Baseline-fallback resolution (CONTRACTS.md §5) ──────────────────────
    // Resolve <slot> to the song that actually authors it: <song> itself if
    // it dresses the slot, else the baseline (sonata) if IT dresses the
    // slot, else "" (no song provides it — the caller falls back further to
    // its own facet-side `fallback` Component, or renders nothing).
    function resolveSong(song, slot) {
        if (root.has(song, slot)) return song
        if (root.has(root.baselineSong, slot)) return root.baselineSong
        return ""
    }
}
