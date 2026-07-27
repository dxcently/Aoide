// DesktopGadgets.qml — the floating-gadget desktop layer (Win7 "tear-off").
//
// Windows 7 let you drag a sidebar gadget out onto the desktop. This is that
// layer: ONE full-screen, non-exclusive PanelWindow on WlrLayer.Top (namespace
// "aoide-gadgets", transparent) that hosts FLOATING copies of the dock gadgets.
// Each floating gadget is a GadgetFrame + gadget content, DRAGGABLE by its
// title bar and dismissable via a small [x] in the title row.
//
// Input mask (the crux): a full-screen Top window would deaden the whole
// desktop, so the mask is the UNION of the floating gadgets' rects — clicks on
// bare desktop fall through, clicks on a gadget land. The idiom mirrors
// shell.qml's dockWin mask (bound-geometry Region children, zero-size = off):
// a FIXED POOL of Region children, each tracking a Repeater delegate through
// Region.item (which follows an Item's geometry LIVE — including mid-drag). The
// pool reads floatRep.count, so the mask re-forms reactively as gadgets are
// torn off or closed. Empty slots resolve to null → a zero region.
//
// The floating set lives on the shared session object (shell.qml `shared`): a
// ListModel of { kind, gx, gy }. No persistence (v1 session-scoped) — positions
// are seeded from the model, then each delegate owns its own x/y once dragged.
//
// Colors from notes only. GadgetFrame supplies the glass + ASCII chrome.

import QtQuick
import Quickshell
import Quickshell.Wayland

PanelWindow {
    id: root

    // ── Injected dependencies (mirror the dock's notes/bridge/shared) ──────
    required property var notes
    required property var bridge
    required property var shared

    anchors { top: true; bottom: true; left: true; right: true }
    exclusiveZone: 0
    color: "transparent"
    WlrLayershell.layer: WlrLayer.Top
    WlrLayershell.namespace: "aoide-gadgets"

    readonly property int gadgetWidth: 288

    // ── Input mask: union of floating gadget rects ─────────────────────────
    // itemFor(i) reads floatRep.count → the binding re-runs when the floating
    // set changes; Region.item then tracks that delegate's live geometry.
    function itemFor(i) {
        return floatRep.count > i ? floatRep.itemAt(i) : null
    }
    mask: Region {
        Region { item: root.itemFor(0) }
        Region { item: root.itemFor(1) }
        Region { item: root.itemFor(2) }
        Region { item: root.itemFor(3) }
        Region { item: root.itemFor(4) }
        Region { item: root.itemFor(5) }
    }

    // ── Gadget content components (one per tear-off-able kind) ─────────────
    Component {
        id: batonComp
        BatonGadget { notes: root.notes }
    }
    Component {
        id: termComp
        TerminalManagerGadget {
            notes: root.notes; bridge: root.bridge; shared: root.shared
        }
    }
    Component {
        id: dagComp
        DagGraphGadget {
            notes: root.notes; bridge: root.bridge; shared: root.shared
        }
    }

    // ── Floating layer ──────────────────────────────────────────────────────
    Item {
        id: gLayer
        anchors.fill: parent

        Repeater {
            id: floatRep
            model: root.shared ? root.shared.floatingModel : null

            delegate: Item {
                id: fd
                required property int index
                required property string kind
                required property real gx
                required property real gy

                // +depthExtent headroom so the frame's back outline copies
                // (offset down-right) aren't clipped by the delegate bounds;
                // the input-mask region tracks this rect, so it grows to match.
                width: root.gadgetWidth + frame.depthExtent
                height: frame.implicitHeight + frame.depthExtent

                // Seed the position WITHOUT a binding (a binding would fight the
                // drag). After this the delegate owns its own x/y; the mask
                // tracks it live through Region.item.
                Component.onCompleted: { x = gx; y = gy }

                // ── The gadget itself ─────────────────────────────────────
                GadgetFrame {
                    id: frame
                    anchors.left: parent.left
                    anchors.top: parent.top
                    width: root.gadgetWidth
                    notes: root.notes
                    title: fd.kind
                    Loader {
                        width: parent.width
                        sourceComponent: fd.kind === "BATON" ? batonComp
                                       : fd.kind === "TERMINALS" ? termComp
                                       : dagComp
                    }
                }

                // ── Drag by the title bar ─────────────────────────────────
                // Covers the title row; drag.target moves the whole delegate.
                // Declared after `frame` so it stacks above the chrome.
                MouseArea {
                    id: titleDrag
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.top: parent.top
                    height: 22
                    drag.target: fd
                    drag.axis: Drag.XAndYAxis
                    cursorShape: Qt.DragMoveCursor
                }

                // ── Close [x] — top-right, above the drag strip ───────────
                Text {
                    id: closeGlyph
                    anchors.right: parent.right
                    anchors.top: parent.top
                    anchors.rightMargin: 6
                    anchors.topMargin: 4
                    text: "[x]"
                    color: closeMouse.containsMouse ? notes.paletteUrgent
                                                     : notes.paletteAccent
                    opacity: closeMouse.containsMouse ? 1.0 : 0.7
                    font.family: "monospace"
                    font.pixelSize: 12
                    font.bold: true
                }
                MouseArea {
                    id: closeMouse
                    anchors.fill: closeGlyph
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onClicked: {
                        if (root.shared)
                            root.shared.unfloatGadget(fd.index)
                    }
                }
            }
        }
    }
}
