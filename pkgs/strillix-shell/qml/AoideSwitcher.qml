// AoideSwitcher.qml — the plain window switcher, the `switcher` slot's floor.
//
// shell.qml hosts it as the lane-side `fallback` of `SurfaceSlot { slot:
// "switcher" }`: a rice that ships `widgets/switcher.qml` dresses the slot and
// this file is never created; every other rice gets this one. The lane owns the
// one `aoide:switcher` GlobalShortcut (shell.qml) and calls `.toggle()` on
// whichever body the slot holds, so this file registers no shortcut of its own.
//
// Rows are the Hyprland clients (Quickshell.Hyprland's toplevels, read only):
// icon + title, the focused workspace's windows first, each group in focus
// order. Focusing goes out through the bridge (`focuswindow`), never a hyprctl
// dispatch from QML. Keys: ←/→ and Tab/Shift+Tab move (wrapping), Enter or the
// release of the held modifier focuses, Esc cancels. A repeat of the shortcut
// while shown steps to the next row: with the modifier still held, a second
// Super+Tab is a step, as in any alt-tab.
//
// Colours only from theme roles; radius 0; deliberately plain.

import QtQuick
import Quickshell
import Quickshell.Wayland
import Quickshell.Hyprland

PanelWindow {
    id: root

    required property var theme
    required property var bridge

    property var toplevels: Hyprland.toplevels.values
    property int currentWorkspace: Hyprland.focusedWorkspace ? Hyprland.focusedWorkspace.id : -1

    property bool shown: false
    property int sel: 0
    property bool moved: false

    readonly property var rows: {
        var out = []
        var tl = root.toplevels || []
        for (var i = 0; i < tl.length; i++) {
            var t = tl[i]
            if (!t || !t.address) continue
            var ipc = t.lastIpcObject || {}
            var ws = t.workspace ? t.workspace.id : (ipc.workspace ? ipc.workspace.id : -1)
            if (ws < 0) continue
            out.push({
                "address": "0x" + ("" + t.address).replace(/^0x/i, ""),
                "title": t.title || ipc.title || "",
                "cls": ipc.class || "",
                "ws": ws,
                "focus": ipc.focusHistoryID === undefined ? 9999 : ipc.focusHistoryID
            })
        }
        var cur = root.currentWorkspace
        out.sort(function (a, b) {
            var ga = a.ws === cur ? 0 : 1, gb = b.ws === cur ? 0 : 1
            return ga !== gb ? ga - gb : a.focus - b.focus
        })
        return out
    }

    function initialSel() { return root.rows.length > 1 && root.rows[0].focus === 0 ? 1 : 0 }

    function show() {
        Hyprland.refreshToplevels()
        root.moved = false
        root.sel = root.initialSel()
        root.shown = true
    }
    function hide() { root.shown = false }
    function toggle() { if (root.shown) root.move(1); else root.show() }

    function move(delta) {
        var n = root.rows.length
        if (n === 0) return
        root.moved = true
        root.sel = ((root.sel + delta) % n + n) % n
    }
    function activate() {
        var r = root.rows[root.sel]
        if (r) root.bridge.focusWindow(r.address)
        root.hide()
    }

    onRowsChanged: if (root.shown && !root.moved) root.sel = root.initialSel()
    onShownChanged: if (root.shown) keys.forceActiveFocus()

    anchors { top: true; bottom: true; left: true; right: true }
    exclusiveZone: 0
    color: "transparent"
    visible: root.shown
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "aoide-switcher"
    WlrLayershell.keyboardFocus: root.shown ? WlrKeyboardFocus.Exclusive : WlrKeyboardFocus.None

    MouseArea { anchors.fill: parent; onClicked: root.hide() }

    Rectangle {
        anchors.centerIn: parent
        width: Math.min(root.width - 80, Math.max(240, list.contentWidth + 24))
        height: 132
        radius: 0
        color: root.theme.paletteBg
        border.color: root.theme.paletteFg
        border.width: 1

        Item {
            id: keys
            anchors.fill: parent
            focus: true
            Keys.onPressed: function (event) {
                if (event.key === Qt.Key_Right || event.key === Qt.Key_Tab) root.move(1)
                else if (event.key === Qt.Key_Left || event.key === Qt.Key_Backtab) root.move(-1)
                else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) root.activate()
                else if (event.key === Qt.Key_Escape) root.hide()
                else return
                event.accepted = true
            }
            Keys.onReleased: function (event) {
                if (event.isAutoRepeat || !root.shown) return
                if (event.key === Qt.Key_Super_L || event.key === Qt.Key_Super_R
                        || event.key === Qt.Key_Meta || event.key === Qt.Key_Alt) {
                    root.activate()
                    event.accepted = true
                }
            }
        }

        Text {
            anchors.centerIn: parent
            visible: root.rows.length === 0
            text: "no windows"
            color: root.theme.paletteFg
            opacity: 0.5
            font.family: "monospace"
            font.pixelSize: 13
        }

        ListView {
            id: list
            anchors.fill: parent
            anchors.margins: 12
            orientation: ListView.Horizontal
            clip: true
            model: root.rows
            currentIndex: root.sel
            highlightFollowsCurrentItem: false
            onCurrentIndexChanged: list.positionViewAtIndex(currentIndex, ListView.Contain)

            delegate: Rectangle {
                id: cell
                required property var modelData
                required property int index
                width: 120
                height: list.height
                radius: 0
                color: "transparent"
                border.color: root.sel === cell.index ? root.theme.paletteAccent : "transparent"
                border.width: 1

                readonly property var entry: cell.modelData.cls ? DesktopEntries.heuristicLookup(cell.modelData.cls) : null

                Image {
                    anchors.horizontalCenter: parent.horizontalCenter
                    y: 14
                    width: 48
                    height: 48
                    sourceSize.width: 48
                    sourceSize.height: 48
                    source: Quickshell.iconPath(cell.entry && cell.entry.icon ? cell.entry.icon : cell.modelData.cls, true)
                            || Quickshell.iconPath("application-x-executable", true)
                }
                Text {
                    anchors { left: parent.left; right: parent.right; bottom: parent.bottom; margins: 8 }
                    text: cell.modelData.title
                    color: root.theme.paletteFg
                    font.family: "monospace"
                    font.pixelSize: 11
                    elide: Text.ElideRight
                    horizontalAlignment: Text.AlignHCenter
                }
                MouseArea {
                    anchors.fill: parent
                    onClicked: { root.sel = cell.index; root.activate() }
                }
            }
        }
    }
}
