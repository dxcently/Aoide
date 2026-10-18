// calendar.qml — this song's "calendar" surface: the calendar body on a layer
// surface of its own, summoned by the bar's clock through the bridge
// (`strillix component calendar toggle <screen>`).
//
// The clock measures its cell into shared.calendarAnchor (screen coordinates)
// before it asks, and this window pins under it. The body is CalendarBody.qml.
// The open state goes back out on shared.calendarOpen.

import QtQuick
import Quickshell
import Quickshell.Wayland

PanelWindow {
    id: root

    required property var theme
    required property var bridge
    property var shared: null
    property bool shown: false

    function show() { root.shown = true }
    function hide() { root.shown = false }
    // The screen the bridge names is the one the calendar lands on; an unknown
    // name keeps the screen it has.
    function toggle(screenName) {
        var screens = Quickshell.screens
        for (var i = 0; i < screens.length; i++)
            if (screens[i].name === screenName) root.screen = screens[i]
        root.shown = !root.shown
    }

    visible: shown
    color: "transparent"
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "aoide-calendar"
    exclusionMode: ExclusionMode.Ignore
    anchors { top: true; left: true }
    margins.left: root.shared ? Math.round(root.shared.calendarAnchor.x) : 0
    margins.top: root.shared ? Math.round(root.shared.calendarAnchor.y) : 0
    implicitWidth: body.implicitWidth
    implicitHeight: body.implicitHeight
    onShownChanged: if (root.shared) root.shared.calendarOpen = root.shown

    CalendarBody { id: body; theme: root.theme; bridge: root.bridge }
}
