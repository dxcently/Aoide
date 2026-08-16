// SurfaceSlot.qml — the fixed per-slot anchor for slots that own their own
// window (powermenu, launcher), the SurfaceSlot analogue of WidgetSlot.qml.
//
// WidgetSlot is an `Item`: it sizes itself off the loaded child and parents
// it into an existing layout. That doesn't work for a slot whose root IS a
// `PanelWindow` — its own Overlay layer surface, own WlrLayershell namespace,
// own keyboard focus, own GlobalShortcut. SurfaceSlot is a non-visual
// `QtObject` sibling instead: same `resolveSong` → `Qt.createComponent` →
// `Component.createObject(parent, initialProperties)` mechanism WidgetSlot
// already uses (same rationale — QML resolves `required property` at OBJECT
// CREATION, so a declarative `Loader` is too late for required props), but
// with no visual parenting/sizing: just `item` holding whatever got created,
// a top-level Window with no Item parent (matching how AoideExodos/
// AoideLauncher are declared today, directly under ShellRoot with no Item
// ancestor of their own).
//
// ── Component.createObject + PanelWindow grounding (real, not guessed) ─────
// Checked against the compiled Quickshell 0.3.0 plugin's own .qmltypes
// before writing this file: `PanelWindow` (as imported via
// `Quickshell.Wayland`/`Quickshell`) is NOT a C++-registered QML type in any
// `.qmltypes` — it isn't in quickshell-core.qmltypes, quickshell-wayland
// .qmltypes, or anywhere else the public symbol would show up. The only
// `PanelWindow`-named C++ registration anywhere in the plugin is the
// internal `Quickshell._Window/PanelWindow` (`PanelWindowInterface`,
// panelinterface.hpp), marked `isCreatable: false` — but that's the
// abstract low-level interface every already-shipped DECLARATIVE
// `PanelWindow { ... }` root in this codebase (AoideExodos.qml,
// AoideLauncher.qml, AoideNotifications.qml) is ALSO built on; if that flag
// blocked instantiation of the public type, those would never have worked
// either. The public `PanelWindow` is QML-composite (a `.qml` file bundled
// into the plugin's Qt resources, wrapping the interface), not a
// C++-restricted singleton — so it follows the ordinary QML rule: any
// composite type defined by a `.qml` file is creatable via
// `Component.createObject()` exactly like a plain `Item`-rooted one, the
// same standard pattern Qt's own docs use for dynamically creating
// `Window`-derived types (dialogs, popup windows, …).
//
// This grounding was NOT confirmed by an actual live `qs -p` run against a
// running compositor — that verification is still owed by a human or a
// follow-up pass with desktop access (see the throwaway smoke-test harness
// left in the scratchpad this pass produced). Flagged explicitly, not
// silently assumed away.

import QtQuick

QtObject {
    id: root

    required property var notes
    required property var bridge
    required property var stagingEngine
    required property string slot
    property var extraProps: ({})

    // Same baseline-fallback resolution WidgetSlot uses (CONTRACTS.md §5):
    // the active song if it authored `slot`, else sonata. No facet-side
    // `fallback` Component here — unlike calendar/notifications, a
    // window-owning slot has no shared PanelWindow left behind in the facet
    // to fall back to; powermenu/launcher move as whole units, sonata IS
    // the floor.
    readonly property string resolvedSong: stagingEngine.resolveSong(notes.songName, slot)
    readonly property bool songProvides: resolvedSong !== ""
    readonly property string resolvedSource:
        songProvides ? stagingEngine.source(resolvedSong, slot) : ""

    // Whatever got created — a top-level Window (PanelWindow), never an
    // Item, so typed `var` rather than `Item`. null until the first
    // successful create, and again whenever resolution comes up empty.
    property var item: null

    // ── What the live `item` was built from — the idempotence key ──────────
    // (khoa, 2026-08-15) FOUR separate triggers re-enter `_rebuild()` on
    // every single reload, in this order (traced live, one full reload):
    //   extraProps → onCompleted → resolvedSource → manifestChanged
    // The first two land while `stagingEngine.manifest` is still `{}`, so
    // they only log "no song provides slot". The third builds the surface.
    // The FOURTH — the belt-and-suspenders manifest watch below — then found
    // `item` already SET and unconditionally tore it down and built a second,
    // identical one. Every reload therefore constructed and destroyed a whole
    // spare PanelWindow.
    //
    // That spare destroy is what floods the journal. `item.destroy()` deletes
    // the window while the QML engine is fully live, which nulls the `root`
    // id inside the loaded widget's own context and re-evaluates every
    // binding that captured it — `launcher.qml` alone has 65 `root.notes.*`
    // bindings, so one spare destroy = a ~66-79-line burst of
    // "TypeError: Cannot read property 'notes' of null" (@songs/sonata/
    // launcher.qml, 1984 of them in six hours). `powermenu.qml` is the same
    // shape and stays quiet only because it is fully static — no Repeater or
    // ListView delegates to re-evaluate on the way down.
    //
    // So the fix is not a null guard in the widget: it is not rebuilding what
    // did not change. A trigger whose source AND extras match what `item` was
    // already built from now returns without touching it. A GENUINE change
    // (live `aoide rice preview <song>`, a new extra) still rebuilds — the
    // manifest watch keeps doing the job it was added for.
    property string _builtSource: ""
    property var _builtExtras: ({})

    onResolvedSourceChanged: _rebuild()
    onExtraPropsChanged: _rebuild()
    Component.onCompleted: _rebuild()

    // Explicit re-trigger on the manifest's own load, not just a transitive
    // dependency through resolvedSong/resolvedSource. QtObject has no
    // default child property (same reason manifestFile above is a named
    // property, not a bare child — StagingEngine.qml's own precedent), so
    // this is declared the same way. Belt-and-suspenders: confirmed live
    // that Component.onCompleted fires with stagingEngine.manifest still
    // `{}` (the FileView's async reload hasn't landed yet), and the
    // resolvedSong/resolvedSource chain's transitive reactivity to that
    // later change was NOT reliably re-triggering _rebuild() for this
    // (QtObject-rooted, no visual tree) component the way it does for
    // WidgetSlot's Item-rooted one — rather than chase why, force the
    // dependency explicitly so correctness never depends on it.
    property Connections _manifestWatch: Connections {
        target: root.stagingEngine
        function onManifestChanged() { root._rebuild() }
    }

    function _rebuild() {
        // Idempotent (see `_builtSource` above): the live surface already came
        // from exactly this source and these extras, so there is nothing to
        // rebuild — and tearing it down to build an identical one is what
        // floods the journal.
        if (root.item && root.resolvedSource === root._builtSource
                && root._sameExtras(root.extraProps)) return
        if (root.item) {
            root.item.destroy()
            root.item = null
        }
        root._builtSource = root.resolvedSource
        root._builtExtras = root.extraProps
        if (root.songProvides) {
            var comp = Qt.createComponent(root.resolvedSource)
            root._create(comp)
        } else {
            console.warn("[aoide/surfaceslot] no song (active or baseline) provides slot", root.slot)
        }
    }

    // Shallow value compare, not identity: `extraProps` is declared in
    // shell.qml as an object LITERAL binding (`({ clipboard: …, ledger: … })`),
    // so every re-evaluation hands over a brand-new JS object holding the very
    // same instances. Comparing by identity would call that a change and
    // rebuild forever.
    function _sameExtras(next) {
        var prev = root._builtExtras
        if (!prev || !next) return prev === next
        var kn = Object.keys(next)
        var kp = Object.keys(prev)
        if (kn.length !== kp.length) return false
        for (var i = 0; i < kn.length; i++) {
            if (next[kn[i]] !== prev[kn[i]]) return false
        }
        return true
    }

    function _props() {
        var base = { "notes": root.notes, "bridge": root.bridge }
        var keys = Object.keys(root.extraProps)
        for (var i = 0; i < keys.length; i++) base[keys[i]] = root.extraProps[keys[i]]
        return base
    }

    function _create(comp) {
        if (!comp) return
        if (comp.status === Component.Loading) {
            comp.statusChanged.connect(function () { root._finish(comp) })
            return
        }
        root._finish(comp)
    }

    function _finish(comp) {
        if (comp.status === Component.Error) {
            console.warn("[aoide/surfaceslot] failed to load slot", root.slot, "-", comp.errorString())
            return
        }
        if (comp.status !== Component.Ready) return
        // null parent — the created PanelWindow is a top-level surface, not
        // a visual child to parent into a layout (the WidgetSlot precedent
        // parents into itself because it IS the layout slot; SurfaceSlot has
        // no layout role at all). Matches how AoideExodos/AoideLauncher are
        // declared today: direct ShellRoot children, no Item ancestor.
        var item = comp.createObject(null, root._props())
        if (!item) {
            console.warn("[aoide/surfaceslot] createObject failed for slot", root.slot)
            return
        }
        root.item = item
    }
}
