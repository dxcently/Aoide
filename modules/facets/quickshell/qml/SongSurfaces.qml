// SongSurfaces.qml — non-visual host for the active song's DECLARED
// surface-kind widgets (CONTRACTS.md §5's "Per-song flavor widgets";
// `aoide.arrangement.widgets`, `modules/nucleus/options.nix` — this is the
// QML runtime half, Phase 4, of a feature whose nix option + build-time
// registry.json walk (Phase 1/2) and `rice stage`/`preview` runtime
// hot-sync + `rice lint` validation (Phase 3) already landed).
//
// For the ACTIVE song (`notes.songName`) and every entry in
// `stagingEngine.declaredWidgets(notes.songName)` whose `kind === "surface"`,
// stands up one real SurfaceSlot — the same non-visual `QtObject` anchor
// CONTRACTS.md §5 already defines for slots that own their own PanelWindow
// (powermenu/launcher, shell.qml). Reused directly, not duplicated: same
// `resolveSong` → `Qt.createComponent` → `Component.createObject` mechanism,
// same baseline-fallback chain (active song, else sonata), same
// `[aoide/surfaceslot]` console.warn when neither the active song nor
// sonata actually carries a committed `widgets/<slot>.qml` body for a
// declared slot — a declared-but-bodyless slot therefore already warns and
// renders nothing for free, no extra plumbing needed here.
//
// Instantiator (QtQml.Models) keyed on the declared-widgets list, so a live
// song switch (`aoide rice preview <name>`) tears down the old song's
// entries and stands up the new song's — the same reactive contract
// WidgetSlot.resolvedSource already gives per-slot (CONTRACTS.md's
// "Additive 2026-08-14 baseline-fallback resolution"). No other precedent
// for a dynamic list of non-visual QtObjects exists in this codebase (every
// existing Repeater in qml/ is a visual delegate list) — Instantiator is
// the QML-native tool for this shape, same idiom other Quickshell configs
// use for per-monitor/per-workspace non-visual objects.
//
// Fixed injected-prop contract (CONTRACTS.md §5 containment): unchanged —
// SurfaceSlot itself injects only `notes` + `bridge` into the loaded
// widget body. Nothing here adds extras.
//
// Optional shortcut (options.nix `widgetType.shortcut`, e.g. "aoide:grimoire"):
// one Hyprland GlobalShortcut per entry that declares a non-null shortcut,
// split on ":" into appid/name (the same "appid, name" pair every other
// GlobalShortcut in this codebase declares — AoidePanel's aoide:dock,
// AoideWallpaperPicker's aoide:wallpaper), calling `.toggle()` on the
// loaded item IF it exposes one — same pattern CONTRACTS.md documents for
// "the bar's clef calling `.item.toggle()`" on the powermenu slot, guarded
// here since a declared widget isn't required to expose `toggle()`. A
// nested Instantiator (`active` gated on the shortcut being non-null, not a
// ternary object literal — QML has no conditional object-literal syntax)
// keeps an unshortcut'd entry from registering a bogus empty keybind.

import QtQuick
import QtQml.Models
import Quickshell.Hyprland

QtObject {
    id: root

    required property var notes
    required property var bridge
    required property var stagingEngine

    // [{ slot, decl }, …] — every surface-kind entry the active song
    // declares. Empty for a song with no `.widgets` (sonata today, the only
    // committed song) — the Instantiator below then holds zero delegates: a
    // structural no-op, not an accident of which song happens to be active.
    readonly property var surfaceEntries: {
        var decl = root.stagingEngine.declaredWidgets(root.notes.songName)
        var keys = Object.keys(decl)
        var out = []
        for (var i = 0; i < keys.length; i++) {
            var slot = keys[i]
            var w = decl[slot]
            if (w && w.kind === "surface") out.push({ slot: slot, decl: w })
        }
        return out
    }

    // Declared as a property (QtObject has no default property — the
    // manifestFile/registryFile idiom StagingEngine.qml already uses).
    property Instantiator _hosts: Instantiator {
        model: root.surfaceEntries
        delegate: QtObject {
            id: entry

            required property string slot
            required property var decl

            property SurfaceSlot surfaceSlot: SurfaceSlot {
                notes: root.notes
                bridge: root.bridge
                stagingEngine: root.stagingEngine
                slot: entry.slot
            }

            // Conditional shortcut registration: `active` gates the whole
            // nested Instantiator off rather than a ternary, since QML has
            // no way to inline a conditional object literal into a
            // property binding.
            property Instantiator _shortcutHost: Instantiator {
                active: !!entry.decl.shortcut
                model: 1
                delegate: GlobalShortcut {
                    readonly property string _raw: entry.decl.shortcut || ""
                    readonly property int _sep: _raw.indexOf(":")
                    appid: _sep >= 0 ? _raw.substring(0, _sep) : _raw
                    name: _sep >= 0 ? _raw.substring(_sep + 1) : ""
                    description: "Declared widget shortcut: " + entry.slot
                    onPressed: {
                        var item = entry.surfaceSlot.item
                        if (item && item.toggle) item.toggle()
                    }
                }
            }
        }
    }
}
