// WidgetSlot.qml — the fixed per-slot anchor: song override, baseline
// fallback, shared fallback.
//
// One WidgetSlot sits wherever a host surface (AoideBar's calendar popout,
// AoideNotifications' card repeater, …) wants to let the active song replace
// a piece of chrome with its own QML. It asks the staging engine to resolve
// `slot` against a baseline-fallback chain (CONTRACTS.md §5): the active
// song (`livery.songName`) if it authored the slot, else sonata (the shipped
// baseline every song can fall back to), else the anchor's own lane-side
// `fallback` Component, else nothing.
//
// Fixed injected-prop contract (CONTRACTS.md §5 containment): whatever loads
// — song override or fallback — receives `livery` always, `bridge` only when
// it actually declares that property, plus each key of `extraProps`
// (slot-specific extras, e.g. notifications' `notification`). Never nix
// `config.*` — a song widget is store-copied score, structurally incapable
// of reaching host/lane options through this surface. Both slots in scope
// this pass (calendar's proof stubs, per the fixed contract) declare both
// `livery` and `bridge` as `required` even though bridge goes unused today.
//
// Deliberately NOT a declarative `Loader { source: … }`/`sourceComponent`:
// QML resolves `required property` at OBJECT CREATION — a Loader only lets
// you assign properties on the item AFTER it loads (in `onLoaded`), which is
// too late to satisfy a required property (creation throws first, so the
// item never becomes non-null and `onLoaded` never fires). The QML-supported
// way to hand initial values — including required ones — to a dynamically
// created object is `Component.createObject(parent, initialProperties)`, so
// this manages the loaded item's lifecycle directly instead.
//
// Rebuild key and extras: the loaded widget is rebuilt only when the resolved
// source changes (`_rebuild()` below). Extras are not part of that key. An
// `extraProps` binding is an object literal — a fresh JS object on every
// re-evaluation, same instances inside — and an extra can itself change while
// a song switch is in flight (the bar's `powermenu`/`dock` are the live
// `.item` of another slot, null while that slot rebuilds). Extras are assigned
// onto the live widget key by key (an identical value signals nothing), so a
// widget reads an extra at use time and never assumes the value it was
// created with.
import QtQuick

Item {
    id: root

    required property var livery
    required property var bridge
    required property var stagingEngine
    required property string slot
    property var extraProps: ({})
    property Component fallback: null

    // Reactive off livery.songName (so live `aoide rice preview <name>` swaps
    // the loaded widget with no restart) and off stagingEngine.manifest (so a
    // rebuild's new manifest is picked up). Resolves through the baseline
    // chain (CONTRACTS.md §5): the active song if it authored `slot`, else
    // sonata (the baseline), else "" — an empty resolvedSong means neither
    // the active song nor the baseline provide it, so `_rebuild` falls
    // through to the lane-side `fallback` Component.
    readonly property string resolvedSong: stagingEngine.resolveSong(livery.songName, slot)
    readonly property bool songProvides: resolvedSong !== ""

    // The resolved widget URL, or "" when no song (active or baseline)
    // provides this slot. Rebuild keys off THIS, not just `songProvides` —
    // two different songs can both provide the same slot (`songProvides`
    // stays true→true across a preview switch, or across the active song
    // falling through to the baseline), so a bool-only trigger would
    // silently keep rendering the FIRST resolved widget forever. Reacting to
    // the actual resolved path catches that case too.
    readonly property string resolvedSource:
        songProvides ? stagingEngine.source(resolvedSong, slot) : ""

    property Item _item: null

    // The resolved source the live `_item` was built from. `_rebuild` tears
    // down and reloads only when it differs. Covers both branches below: the
    // song-provides branch keys off `resolvedSource` (non-empty while a song
    // owns the slot), and the `fallback` branch keys off the same field
    // staying `""` (its stable value whenever `songProvides` is false).
    property string _builtSource: ""

    // Hosts size to content (BarPopout's slot Item uses
    // implicitHeight: childrenRect.height; a Repeater delegate binds its own
    // width) — passthrough from whatever actually loaded.
    implicitWidth: _item ? _item.implicitWidth : 0
    implicitHeight: _item ? _item.implicitHeight : 0
    width: implicitWidth
    height: implicitHeight

    onResolvedSourceChanged: _rebuild()
    onExtraPropsChanged: root._applyExtras(root._item)
    Component.onCompleted: _rebuild()

    function _rebuild() {
        if (root._item && root.resolvedSource === root._builtSource) return
        if (root._item) {
            root._item.destroy()
            root._item = null
        }
        root._builtSource = root.resolvedSource
        if (root.songProvides) {
            var comp = Qt.createComponent(root.stagingEngine.source(root.resolvedSong, root.slot))
            root._create(comp, root._songProps())
        } else if (root.fallback) {
            root._create(root.fallback, root._fallbackProps())
        }
    }

    function _applyExtras(item) {
        if (!item) return
        var keys = Object.keys(root.extraProps)
        for (var i = 0; i < keys.length; i++) {
            if (item.hasOwnProperty(keys[i])) item[keys[i]] = root.extraProps[keys[i]]
        }
    }

    function _songProps() {
        return root._merge({ "livery": root.livery, "bridge": root.bridge })
    }

    function _fallbackProps() {
        // bridge deliberately omitted here — the shared fallback isn't
        // contractually required to declare it; guarded post-creation
        // assignment below covers the case where it does.
        return root._merge({ "livery": root.livery })
    }

    function _merge(base) {
        var keys = Object.keys(root.extraProps)
        for (var i = 0; i < keys.length; i++) base[keys[i]] = root.extraProps[keys[i]]
        return base
    }

    function _create(comp, props) {
        if (!comp) return
        if (comp.status === Component.Loading) {
            comp.statusChanged.connect(function () { root._finish(comp, props) })
            return
        }
        root._finish(comp, props)
    }

    function _finish(comp, props) {
        if (comp.status === Component.Error) {
            console.warn("[aoide/widgetslot] failed to load slot", root.slot, "-", comp.errorString())
            return
        }
        if (comp.status !== Component.Ready) return
        var item = comp.createObject(root, props)
        if (!item) {
            console.warn("[aoide/widgetslot] createObject failed for slot", root.slot)
            return
        }
        // bridge wasn't in the fallback's creation props — inject it now,
        // but only if the item actually declares that property.
        if (props.bridge === undefined && item.hasOwnProperty("bridge")) item.bridge = root.bridge
        // `props` was captured before the component finished loading; an extra
        // that changed meanwhile found no `_item` to receive it.
        root._applyExtras(item)
        root._item = item
    }
}
