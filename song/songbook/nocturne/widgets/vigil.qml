// vigil.qml — nocturne's "vigil" slot: the ONE declared kind = "dock" widget
// this throwaway demo song authors (CONTRACTS.md §5's declared widget-type
// registry — Phase 12's live proof that a `kind = "dock"` entry actually
// mounts). Hosted by SongGadgets.qml's data-driven Repeater of WidgetSlots
// (not a fixed AoidePanel.qml anchor), mounted last in the gadget column,
// after the six shipped gadgets and herald-center.
//
// Shape mirrors sonata's widgets/herald-center.qml (Item root, self-sizing
// via width/implicitHeight, only notes+bridge injected — no extraProps),
// the same fixed slot contract every widget in the wired catalog follows.
//
// Content is deliberately small and unmistakable: a single dark-palette
// panel labelled NOCTURNE, so a screenshot makes it obvious at a glance
// which gadget is the newly-declared one versus the shipped six.

import QtQuick

Item {
    id: root

    required property var notes
    required property var bridge   // unused today — fixed slot contract

    width: parent ? parent.width : 360
    implicitHeight: panel.height

    Rectangle {
        id: panel
        width: parent.width
        height: label.implicitHeight + 20
        radius: 6
        color: root.notes.paletteBg
        border.color: root.notes.paletteAccent
        border.width: 2

        Text {
            id: label
            anchors.centerIn: parent
            text: "☾ NOCTURNE"
            font.family: "Noto Serif"
            font.pixelSize: 15
            font.weight: Font.Bold
            font.letterSpacing: 3
            color: root.notes.paletteAccent
        }
    }
}
