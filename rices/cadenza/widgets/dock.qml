// dock.qml — cadenza's "dock" slot: the BOARD's window (intent §3.3).
//
// A THIN PanelWindow shell (SurfaceSlot host, shell.qml's `dockSlot`): the
// layer, namespace, keyboard focus, input mask and the toggle contract live
// here; everything drawn lives in BoardBody.qml (an Item, so the preview
// canvas — which refuses window roots — can show it), loaded BY URL
// (design/kit.md §1: nothing in cadenza is resolved by type name).
//
// ── The contract (slots.md, shell.qml) ───────────────────────────────────
//   namespace `aoide-dock`, layer Overlay; extras `shared`, `stagingEngine`
//   toggle()          shell.qml's SUPER+G (`aoide:dock`) and the bar
//   openTab(tab)      the bar's cells: "overview" | "sys" | "notif" |
//                     "project:<name>" — opens the board on that tab; on an
//                     open board already on that tab it closes it
//
// ── Closing ──────────────────────────────────────────────────────────────
// Escape (the body), the `[x]` in the board's top rule (the body's
// closeRequested), SUPER+G, the bar cell of the tab already showing, and a
// click anywhere off the board: `catcher` below, a transparent full-screen
// layer (`aoide-dock-scrim`) one layer under the board and clear of the
// bar's zone, mapped only while the board is shown.
//   open              readable: is the board up
//   show() / hide()   the same pair sonata's dock exposes
//
// ── Mapping ──────────────────────────────────────────────────────────────
// rice.nix declares the dock a persistently mapped surface
// (`arrangement.surfaces.dock`), so, like sonata's, the window stays mapped
// and closing is paint: the body's pane runs its close, the input mask drops
// to nothing (clicks fall through to the windows under it) and the keyboard
// is released. Right edge, top-to-bottom; `exclusiveZone: 0` keeps it inside
// the bar's reserved zone, so it hangs full height UNDER the bar. The screen
// follows the focused monitor exactly as sonata's dock does (null → the
// compositor places it).
import QtQuick
import Quickshell
import Quickshell.Wayland
import Quickshell.Hyprland
import "Kit.js" as Kit

PanelWindow {
    id: root

    required property var livery
    required property var bridge
    property var shared: null
    property var stagingEngine: null

    property bool shown: false
    readonly property bool open: root.shown
    property string _pendingTab: ""

    function show()   { root.shown = true }
    function hide()   { root.shown = false }
    function toggle() { root.shown = !root.shown }
    // a bar cell toggles: the open board already on that tab closes; on
    // another tab it switches; a shut board opens on the tab
    function openTab(tab) {
        if (root.shown && body.item && body.item.tab === "" + tab) { root.hide(); return }
        if (body.item) body.item.openTab(tab)
        else root._pendingTab = "" + tab
        root.shown = true
    }

    anchors { top: true; right: true; bottom: true }
    screen: {
        var mon = Hyprland.focusedMonitor
        if (!mon) return null
        var screens = Quickshell.screens
        for (var i = 0; i < screens.length; i++)
            if (screens[i].name === mon.name) return screens[i]
        return null
    }
    exclusiveZone: 0
    color: "transparent"
    visible: true

    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "aoide-dock"
    WlrLayershell.keyboardFocus: root.shown ? WlrKeyboardFocus.OnDemand : WlrKeyboardFocus.None

    implicitWidth: body.item ? body.item.implicitWidth : 560

    // shut: an empty mask, so the mapped-but-closed board deadens nothing
    mask: Region { item: hit }
    Item {
        id: hit
        anchors { top: parent.top; bottom: parent.bottom; right: parent.right }
        width: root.shown ? parent.width : 0
    }

    Loader {
        id: body
        anchors.fill: parent
        Component.onCompleted: setSource(Kit.helper("BoardBody"), {
            livery: Qt.binding(() => root.livery),
            bridge: Qt.binding(() => root.bridge),
            shared: Qt.binding(() => root.shared),
            stagingEngine: Qt.binding(() => root.stagingEngine),
            open: Qt.binding(() => root.shown)
        })
        onLoaded: {
            if (root._pendingTab !== "") { body.item.openTab(root._pendingTab); root._pendingTab = "" }
            if (root.shown) body.item.forceActiveFocus()
        }
    }
    Connections {
        target: body.item
        ignoreUnknownSignals: true
        function onCloseRequested() { root.hide() }
    }
    onShownChanged: if (root.shown && body.item) body.item.forceActiveFocus()

    // ── the catcher: a click off the board closes it ─────────────────────
    // A second layer surface, mapped only while the board is shown: full
    // screen on the board's output, one layer BELOW it (Top; the board is
    // Overlay), transparent, drawing nothing, taking every pointer button.
    // `exclusiveZone: 0` keeps it out of the bar's reserved zone, so the bar
    // stays clickable (a bar cell reaches `openTab` and toggles). The click
    // that closes is consumed: it does not reach the window under it.
    PanelWindow {
        id: catcher
        screen: root.screen
        visible: root.shown
        anchors { top: true; bottom: true; left: true; right: true }
        exclusiveZone: 0
        color: "transparent"
        WlrLayershell.layer: WlrLayer.Top
        WlrLayershell.namespace: "aoide-dock-scrim"
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
        MouseArea {
            anchors.fill: parent
            acceptedButtons: Qt.AllButtons
            onPressed: root.hide()
        }
    }
}
