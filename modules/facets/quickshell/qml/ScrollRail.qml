// ScrollRail.qml — the slim DRAGGABLE scrollbar the dock and the roster steles
// share (dock body · Conductor playbill · Terminals roster).
//
// Draws a track plus a proportional thumb for a Flickable, and takes the
// pointer: press the thumb and it follows the drag; press the bare track and
// the view jumps there (thumb centred under the press) and keeps following
// until release. The wheel and the flick still work exactly as before — this
// only ADDS a grab.
//
// ── Why the lane is wider than the rail ─────────────────────────────────────
// The drawn rail stays hairline-thin (3–4px, the temple's chrome grammar), but
// a 3px target is not catchable with a pointer. The MouseArea therefore spills
// `grabPad` past each side; it draws nothing, so the chrome is unchanged. Hosts
// pass a pad that fits their gutter — never so wide it swallows clicks meant
// for the content beside it.
//
// ── Why preventStealing ─────────────────────────────────────────────────────
// The roster rails sit OVER their own Flickable (the dock's sits beside it).
// Without preventStealing a vertical drag on the thumb is stolen by the
// Flickable underneath after a few pixels, and the view then scrolls twice —
// once from the drag mapping, once from the steal.
//
// Position mapping is `travel`-based (thumb TOP over `height - thumbH`), not
// `visibleArea.yPosition`-based: with a floor under the thumb height the two
// disagree at the bottom of a long stack, and only the travel form lands the
// thumb flush with the end of the track when the content bottoms out.
//
// Colours and geometry come from the host (each wears its own signature and
// anchors its own gutter); nothing here reads `notes` — it is pure chrome.

import QtQuick

Item {
    id: rail

    required property Flickable flick

    property real railW: 4        // the DRAWN width
    property real minThumb: 24    // floor, so a long stack keeps a grabbable thumb
    property real grabPad: 5      // invisible hit lane either side of the rail
    property color trackColor
    property color thumbColor

    // Live while the rail holds the pointer — a host that auto-hides (the dock)
    // watches this so a drag can't slide the surface shut mid-gesture.
    readonly property alias dragging: grab.pressed

    width: rail.railW
    visible: rail.flick.contentHeight > rail.flick.height + 1

    readonly property real span: Math.max(0, rail.flick.contentHeight - rail.flick.height)
    readonly property real thumbH: Math.max(rail.minThumb,
                                            Math.min(rail.height,
                                                     rail.flick.visibleArea.heightRatio * rail.height))
    readonly property real travel: Math.max(0, rail.height - rail.thumbH)

    // map a thumb TOP (rail pixels) back onto the view
    function scrollTo(top) {
        if (rail.span <= 0 || rail.travel <= 0) return
        rail.flick.cancelFlick()
        rail.flick.contentY = rail.flick.originY
                            + Math.max(0, Math.min(1, top / rail.travel)) * rail.span
    }

    Rectangle {                       // the track
        anchors.fill: parent
        color: rail.trackColor
    }

    Rectangle {                       // the thumb
        id: thumb
        y: rail.span > 0
           ? rail.travel * Math.max(0, Math.min(1, (rail.flick.contentY - rail.flick.originY) / rail.span))
           : 0
        height: rail.thumbH
        // at rest exactly the drawn rail; under the pointer it swells a hair
        // into the gutter — the only feedback that it is a handle.
        x: (grab.pressed || grab.containsMouse) ? -1 : 0
        width: (grab.pressed || grab.containsMouse) ? rail.railW + 2 : rail.railW
        color: rail.thumbColor
        Behavior on width { NumberAnimation { duration: 110 } }
        Behavior on x     { NumberAnimation { duration: 110 } }
    }

    MouseArea {
        id: grab
        anchors.fill: parent
        anchors.leftMargin: -rail.grabPad
        anchors.rightMargin: -rail.grabPad
        hoverEnabled: true
        preventStealing: true
        cursorShape: Qt.PointingHandCursor

        // where inside the thumb the press landed, so the thumb doesn't snap
        // its middle to the cursor on the first move
        property real hold: 0

        onPressed: (m) => {
            if (m.y >= thumb.y && m.y <= thumb.y + thumb.height) {
                grab.hold = m.y - thumb.y            // grabbed the thumb
            } else {
                grab.hold = thumb.height / 2         // grabbed bare track — jump here
                rail.scrollTo(m.y - grab.hold)
            }
        }
        onPositionChanged: (m) => { if (grab.pressed) rail.scrollTo(m.y - grab.hold) }
    }
}
