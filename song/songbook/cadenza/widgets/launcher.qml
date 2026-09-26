// launcher.qml — cadenza's "launcher" slot: the THIN window shell.
//
// Hosted by `SurfaceSlot` (modules/facets/quickshell/qml/slots.md): the root
// is a `PanelWindow` owning its layer, namespace, keyboard focus and global
// shortcuts. Everything drawn lives in `LauncherBody.qml`, loaded BY URL
// (design/kit.md §1) — the preview canvas refuses a PanelWindow root, so the
// body is what gets previewed.
//
// Contract (slots.md, the same one sonata's launcher keeps):
//   · namespace `aoide-launcher`, layer Overlay, full-screen, no exclusive
//     zone; keyboard focus Exclusive while shown.
//   · extras `clipboard` (the facet's AoideClipboard) and `ledger` (the
//     facet's GrimoireLedger), handed through to the body untouched.
//   · `toggle()` / `show()` / `hide()` / `openClipboard()`.
//   · GlobalShortcut `aoide:launcher` toggles (the compositor binds
//     SUPER+SPACE to it); `aoide:clipboard` opens straight on the clip tab.
//   · Launch and copy live in the body, by sonata's mechanism
//     (`systemd-run --scope` via execDetached, `clipboard.copyById`).
import QtQuick
import Quickshell
import Quickshell.Wayland
import Quickshell.Hyprland
import "Kit.js" as Kit

PanelWindow {
    id: root

    required property var livery
    required property var bridge
    property var clipboard: null
    property var ledger: null

    property bool shown: false
    property string _openMode: "apps"
    function show()   { root._openMode = "apps"; root.shown = true }
    function hide()   { root.shown = false }
    function toggle() { if (root.shown) root.hide(); else root.show() }
    function openClipboard() {
        root._openMode = "clip"
        if (root.shown) root._prime()
        else root.shown = true
    }

    function _prime() {
        if (!body.item) return
        body.item.reset(root._openMode, "")
        body.item.focusInput()
    }
    onShownChanged: {
        if (!root.shown) { lingerTimer.restart(); return }
        root._prime()
    }
    // stay mapped through the pane's 120ms close
    Timer { id: lingerTimer; interval: 140 }

    anchors { top: true; bottom: true; left: true; right: true }
    exclusiveZone: 0
    color: "transparent"
    visible: root.shown || lingerTimer.running
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "aoide-launcher"
    WlrLayershell.keyboardFocus: root.shown ? WlrKeyboardFocus.Exclusive
                                            : WlrKeyboardFocus.None

    GlobalShortcut {
        appid: "aoide"
        name: "launcher"
        description: "Summon the Aoide app launcher"
        onPressed: root.toggle()
    }
    GlobalShortcut {
        appid: "aoide"
        name: "clipboard"
        description: "Open clipboard history in the launcher"
        onPressed: root.openClipboard()
    }

    Loader {
        id: body
        anchors.fill: parent
        focus: true
        Component.onCompleted: setSource(Kit.helper("LauncherBody"), {
            livery: Qt.binding(() => root.livery),
            bridge: Qt.binding(() => root.bridge),
            clipboard: Qt.binding(() => root.clipboard),
            ledger: Qt.binding(() => root.ledger),
            shown: Qt.binding(() => root.shown) })
        onLoaded: if (root.shown) root._prime()
    }
    Connections {
        target: body.item
        function onDismissed() { root.hide() }
    }
}
