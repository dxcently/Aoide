// powermenu.qml — cadenza's "powermenu" slot: the THIN window shell.
//
// Hosted by `SurfaceSlot` (modules/facets/quickshell/qml/slots.md): the root
// is a `PanelWindow` owning its layer, namespace, keyboard focus and global
// shortcut. Everything drawn lives in `PowerBody.qml`, loaded BY URL
// (design/kit.md §1) — the preview canvas refuses a PanelWindow root, so the
// body is what gets previewed.
//
// Contract (slots.md, the same one sonata's powermenu keeps):
//   · namespace `aoide-powermenu`, layer Overlay, full-screen, no exclusive
//     zone; keyboard focus Exclusive while shown.
//   · `toggle()` / `show()` / `hide()` — the bar's `[⏻]` calls `toggle()`
//     on the slot's live `.item`.
//   · GlobalShortcut `aoide:powermenu` toggles it too.
//   · The body sends `{ cmd: "power", action }` through `bridge.sendCommand`;
//     QML never shells out.
import QtQuick
import Quickshell
import Quickshell.Wayland
import Quickshell.Hyprland
import "Kit.js" as Kit

PanelWindow {
    id: root

    required property var livery
    required property var bridge

    property bool shown: false
    function show()   { root.shown = true }
    function hide()   { root.shown = false }
    function toggle() { root.shown = !root.shown }

    onShownChanged: {
        if (!root.shown) { lingerTimer.restart(); return }
        if (body.item) { body.item.reset(); body.item.forceActiveFocus() }
    }
    // stay mapped through the pane's 120ms close
    Timer { id: lingerTimer; interval: 140 }

    anchors { top: true; bottom: true; left: true; right: true }
    exclusiveZone: 0
    color: "transparent"
    visible: root.shown || lingerTimer.running
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "aoide-powermenu"
    WlrLayershell.keyboardFocus: root.shown ? WlrKeyboardFocus.Exclusive
                                            : WlrKeyboardFocus.None

    GlobalShortcut {
        appid: "aoide"
        name: "powermenu"
        description: "Summon the power menu"
        onPressed: root.toggle()
    }

    Loader {
        id: body
        anchors.fill: parent
        focus: true
        Component.onCompleted: setSource(Kit.helper("PowerBody"), {
            livery: Qt.binding(() => root.livery),
            bridge: Qt.binding(() => root.bridge),
            shown: Qt.binding(() => root.shown) })
    }
    Connections {
        target: body.item
        function onDismissed() { root.hide() }
    }
}
