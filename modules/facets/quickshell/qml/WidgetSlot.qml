// WidgetSlot.qml — the fixed per-slot anchor: song override, shared fallback.
//
// One WidgetSlot sits wherever a host surface (AoideBar's calendar popout,
// AoideNotifications' card repeater, …) wants to let the active song replace
// a piece of chrome with its own QML. It asks the staging engine whether the active
// song (`notes.songName`) authored `slot`; if so it loads that store-copied
// file, else it falls back to the shared `fallback` Component (or renders
// nothing when no fallback is given, e.g. the calendar popout).
//
// Fixed injected-prop contract (CONTRACTS.md §5 containment): whatever loads
// — song override or fallback — receives `notes` always, `bridge` only when
// it actually declares that property, plus each key of `extraProps`
// (slot-specific extras, e.g. notifications' `notification`). Never nix
// `config.*` — a song widget is store-copied score, structurally incapable
// of reaching host/facet options through this surface. Both slots in scope
// this pass (calendar's proof stubs, per the fixed contract) declare both
// `notes` and `bridge` as `required` even though bridge goes unused today.
//
// Deliberately NOT a declarative `Loader { source: … }`/`sourceComponent`:
// QML resolves `required property` at OBJECT CREATION — a Loader only lets
// you assign properties on the item AFTER it loads (in `onLoaded`), which is
// too late to satisfy a required property (creation throws first, so the
// item never becomes non-null and `onLoaded` never fires). The QML-supported
// way to hand initial values — including required ones — to a dynamically
// created object is `Component.createObject(parent, initialProperties)`, so
// this manages the loaded item's lifecycle directly instead.

import QtQuick

Item {
    id: root

    required property var notes
    required property var bridge
    required property var stagingEngine
    required property string slot
    property var extraProps: ({})
    property Component fallback: null

    // Reactive off notes.songName (so live `aoide rice preview <name>` swaps
    // the loaded widget with no restart) and off stagingEngine.manifest (so a
    // rebuild's new manifest is picked up).
    readonly property bool songProvides: stagingEngine.has(notes.songName, slot)

    // The resolved song-widget URL, or "" when no song provides this slot.
    // Rebuild keys off THIS, not just `songProvides` — two different songs
    // can both provide the same slot (`songProvides` stays true→true across
    // a preview switch between them), so a bool-only trigger would silently
    // keep rendering the FIRST song's widget forever. Reacting to the actual
    // resolved path catches that case too.
    readonly property string resolvedSource:
        songProvides ? stagingEngine.source(notes.songName, slot) : ""

    property Item _item: null

    // Hosts size to content (BarPopout's slot Item uses
    // implicitHeight: childrenRect.height; a Repeater delegate binds its own
    // width) — passthrough from whatever actually loaded.
    implicitWidth: _item ? _item.implicitWidth : 0
    implicitHeight: _item ? _item.implicitHeight : 0
    width: implicitWidth
    height: implicitHeight

    onResolvedSourceChanged: _rebuild()
    onExtraPropsChanged: _rebuild()
    Component.onCompleted: _rebuild()

    function _rebuild() {
        if (root._item) {
            root._item.destroy()
            root._item = null
        }
        if (root.songProvides) {
            var comp = Qt.createComponent(root.stagingEngine.source(root.notes.songName, root.slot))
            root._create(comp, root._songProps())
        } else if (root.fallback) {
            root._create(root.fallback, root._fallbackProps())
        }
    }

    function _songProps() {
        return root._merge({ "notes": root.notes, "bridge": root.bridge })
    }

    function _fallbackProps() {
        // bridge deliberately omitted here — the shared fallback isn't
        // contractually required to declare it; guarded post-creation
        // assignment below covers the case where it does.
        return root._merge({ "notes": root.notes })
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
        root._item = item
    }
}
