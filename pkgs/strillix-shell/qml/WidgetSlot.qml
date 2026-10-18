// WidgetSlot.qml — the fixed per-slot anchor: rice override, baseline
// fallback, shared fallback.
//
// One WidgetSlot sits wherever a host surface (AoideBar's calendar popout,
// AoideNotifications' card repeater, …) wants to let the active rice replace
// a piece of chrome with its own QML. It asks the staging engine to resolve
// `slot` against a baseline-fallback chain (CONTRACTS.md §5): the active
// rice (`theme.riceName`) if it authored the slot, else sonata (the shipped
// baseline every rice can fall back to), else the anchor's own lane-side
// `fallback` Component, else nothing.
//
// Fixed injected-prop contract (CONTRACTS.md §5 containment): whatever loads
// — rice override or fallback — receives `theme` always, `bridge` only when
// it actually declares that property, plus each key of `extraProps`
// (slot-specific extras, e.g. notifications' `notification`). Never nix
// `config.*` — a rice widget is store-copied score, structurally incapable
// of reaching host/lane options through this surface. Both slots in scope
// this pass (calendar's proof stubs, per the fixed contract) declare both
// `theme` and `bridge` as `required` even though bridge goes unused today.
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
// a rice switch is in flight (the bar's `powermenu`/`dock` are the live
// `.item` of another slot, null while that slot rebuilds). Extras are assigned
// onto the live widget key by key (an identical value signals nothing), so a
// widget reads an extra at use time and never assumes the value it was
// created with. The widget is tracked as `_item` before any post-creation
// assignment, and a key the widget cannot take (a type mismatch) warns and is
// skipped alone: the widget stays tracked and the other keys still land.
import QtQuick

Item {
    id: root

    required property var theme
    required property var bridge
    required property var stagingEngine
    required property string slot
    property var extraProps: ({})
    property Component fallback: null

    // Reactive off theme.riceName (so live `aoide rice preview <name>` swaps
    // the loaded widget with no restart) and off stagingEngine.manifest (so a
    // rebuild's new manifest is picked up). Resolves through the baseline
    // chain (CONTRACTS.md §5): the active rice if it authored `slot`, else
    // sonata (the baseline), else "" — an empty resolvedRice means neither
    // the active rice nor the baseline provide it, so `_rebuild` falls
    // through to the lane-side `fallback` Component.
    readonly property string resolvedRice: stagingEngine.resolveRice(theme.riceName, slot)
    readonly property bool riceProvides: resolvedRice !== ""

    // The resolved widget URL, or "" when no rice (active or baseline)
    // provides this slot. Rebuild keys off THIS, not just `riceProvides` —
    // two different rices can both provide the same slot (`riceProvides`
    // stays true→true across a preview switch, or across the active rice
    // falling through to the baseline), so a bool-only trigger would
    // silently keep rendering the FIRST resolved widget forever. Reacting to
    // the actual resolved path catches that case too.
    readonly property string resolvedSource:
        riceProvides ? stagingEngine.source(resolvedRice, slot) : ""

    property Item _item: null

    // The resolved source the live `_item` was built from. `_rebuild` tears
    // down and reloads only when it differs. Covers both branches below: the
    // rice-provides branch keys off `resolvedSource` (non-empty while a rice
    // owns the slot), and the `fallback` branch keys off the same field
    // staying `""` (its stable value whenever `riceProvides` is false).
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
        if (root.riceProvides) {
            var comp = Qt.createComponent(root.stagingEngine.source(root.resolvedRice, root.slot))
            root._create(comp, root._riceProps())
        } else if (root.fallback) {
            root._create(root.fallback, root._fallbackProps())
        }
    }

    function _applyExtras(item) {
        if (!item) return
        var keys = Object.keys(root.extraProps)
        for (var i = 0; i < keys.length; i++) {
            if (!item.hasOwnProperty(keys[i])) continue
            try {
                item[keys[i]] = root.extraProps[keys[i]]
            } catch (e) {
                console.warn("[aoide/widgetslot] extra", keys[i], "not assignable on slot", root.slot, "-", e)
            }
        }
    }

    function _riceProps() {
        return root._merge({ "theme": root.theme, "bridge": root.bridge })
    }

    function _fallbackProps() {
        // bridge deliberately omitted here — the shared fallback isn't
        // contractually required to declare it; guarded post-creation
        // assignment below covers the case where it does.
        return root._merge({ "theme": root.theme })
    }

    function _merge(base) {
        var keys = Object.keys(root.extraProps)
        for (var i = 0; i < keys.length; i++) base[keys[i]] = root.extraProps[keys[i]]
        return base
    }

    function _create(comp, props) {
        if (!comp) return
        var source = root._builtSource
        if (comp.status === Component.Loading) {
            comp.statusChanged.connect(function () { root._finish(comp, props, source) })
            return
        }
        root._finish(comp, props, source)
    }

    function _finish(comp, props, source) {
        if (source !== root._builtSource || root._item) return
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
        root._item = item
        // bridge wasn't in the fallback's creation props — inject it now,
        // but only if the item actually declares that property.
        if (props.bridge === undefined && item.hasOwnProperty("bridge")) item.bridge = root.bridge
        // `props` was captured before the component finished loading; an extra
        // that changed meanwhile found no `_item` to receive it.
        root._applyExtras(item)
    }
}
