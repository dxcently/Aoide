// SteleLayerPopout.qml — StelePopout's layer-surface sibling: the same bare
// stele hosting, but on the popout's OWN wlr-layershell surface (own
// namespace) instead of an xdg_popup child of the bar's layer.
//
// WHY THIS EXISTS (khoa, 2026-08-13): Hyprland's `blur_popups` layerrule
// frosts EVERY xdg_popup of the matched layer — and an xdg_popup has no
// namespace of its own, so a single popout cannot opt out of the aoide-bar
// rule while its siblings stay frosted. The sonata calendar's papyrus sheet
// cuts a transparent window over its day grid that must show the desktop
// CRISPLY ("no blur" — owner directive); hosted here on its own namespace
// (default "aoide-calendar", matched by no blur layerrule and pinned
// blur-off compositor-side) it escapes the frost the other bar popouts
// keep. Every other stele popout stays on StelePopout/xdg_popup — this
// host is for a stele that must diverge from the bar layer's compositor
// rules, nothing else.
//
// Positioning: a layer surface cannot anchor to a bar cell the way an
// xdg_popup can, so this pins to the screen's top-left and takes its top-left
// corner from `anchor` (screen coordinates). The caller measures the cell it
// hangs under and sets `anchor` before it shows. Left-pinned by construction —
// the surface grows rightward/downward on resize, which is exactly the anchor
// override the calendar needed from StelePopout (its compact↔expanded morph
// steps the window size; a layer surface also sidesteps the xdg_popup
// resize-remap flicker entirely).
//
// Contract otherwise identical to StelePopout: the child is a self-framed
// stele that sets its own width + implicitHeight; the surface is sized to
// it plus the cast-shadow overhang and the 4px gap under the bar strip.

import QtQuick
import Quickshell
import Quickshell.Wayland

PanelWindow {
    id: root

    // The popout's top-left corner in screen coordinates.
    property point anchor: Qt.point(0, 0)
    property bool shown: false
    // Own namespace = own compositor identity (the whole point — see header).
    property string layerNamespace: "aoide-calendar"
    default property alias content: host.data

    visible: shown
    color: "transparent"

    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: root.layerNamespace
    // Reserve nothing and ignore other exclusive zones: margins are then
    // measured from the true screen edge, matching the bar-scene coords.
    exclusionMode: ExclusionMode.Ignore

    anchors { top: true; left: true }

    margins.left: Math.round(root.anchor.x)
    margins.top: Math.round(root.anchor.y)

    // Sized to the hosted stele (its first child), plus room for the stele's own
    // cast-shadow overhang (4px right / the child's implicitHeight already carries
    // the 5px bottom) and a 4px gap under the bar strip.
    readonly property Item body: host.children.length > 0 ? host.children[0] : null
    implicitWidth: (body ? body.width : 260) + 6
    implicitHeight: (body ? body.implicitHeight : 200) + 8

    Item {
        id: host
        anchors.left: parent.left
        anchors.top: parent.top
        anchors.topMargin: 4
        width: root.body ? root.body.width : parent.width
        implicitHeight: root.body ? root.body.implicitHeight : 0
    }
}
