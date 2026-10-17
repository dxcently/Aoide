// calendar.qml — sonata's "calendar" surface: the papyrus stele on its own
// layer surface, summoned by the bar's clock through the bridge
// (`lyra component calendar toggle <screen>`).
//
// The clock measures its own corner into shared.calendarAnchor before it asks,
// and this window pins under it (SteleLayerPopout). The stele is
// CalendarStele.qml. The open state goes back out on shared.calendarOpen, which
// the clock's lit state reads.

import QtQuick
import Quickshell

SteleLayerPopout {
    id: root

    required property var livery
    required property var bridge
    // shell.qml's shared session state: the clock's anchor, and the open state
    // this surface publishes.
    property var shared: null

    anchor: root.shared ? root.shared.calendarAnchor : Qt.point(0, 0)
    onShownChanged: if (root.shared) root.shared.calendarOpen = root.shown

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

    CalendarStele { livery: root.livery; bridge: root.bridge }
}
