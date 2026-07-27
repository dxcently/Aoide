// TaskbarRow.qml — Win7 taskbar window buttons (the active-window homage).
//
// The Windows 7 taskbar interaction, on the Aoide bar: every open window is
// a squarish glass BUTTON carrying its app icon. The ACTIVE window's button
// is "pressed" — brighter glass fill, accent border, its own gloss sheen —
// while the rest sit flat until hovered (soft glow). Clicking a flat button
// focuses its window; clicking the pressed one minimizes it (the Win7
// toggle); middle-click closes.
//
// Data: Quickshell.Wayland ToplevelManager (zwlr foreign-toplevel) — live
// toplevel list with activate()/close()/minimized, compositor-agnostic (the
// same list Hyprland drives). Icons resolve appId → DesktopEntries
// heuristicLookup → Quickshell.iconPath; windows with no desktop entry (or
// no icon theme on the box) degrade to an initial-letter glyph cell, outline
// treatment matching the bar's text language. All colors from notes.

import QtQuick
import Quickshell
import Quickshell.Wayland

Item {
    id: root
    required property var notes

    readonly property int buttonSize: 28
    readonly property int iconSize: 20

    implicitWidth: row.implicitWidth
    implicitHeight: buttonSize

    Row {
        id: row
        spacing: 3
        anchors.verticalCenter: parent.verticalCenter

        Repeater {
            model: ToplevelManager.toplevels

            delegate: Rectangle {
                id: button
                required property var modelData

                readonly property bool isActive: modelData && modelData.activated
                readonly property bool isMin: modelData && modelData.minimized
                readonly property string appId: (modelData && modelData.appId)
                                                ? ("" + modelData.appId) : ""
                // appId → desktop entry → themed icon path ("" when the box
                // has no matching entry/theme → the initial cell shows).
                readonly property var entry: appId.length > 0
                                             ? DesktopEntries.heuristicLookup(appId) : null
                readonly property string iconSrc: (entry && entry.icon)
                                                  ? Quickshell.iconPath(entry.icon, true) : ""

                width: root.buttonSize + 6
                height: root.buttonSize
                radius: 3

                // ── Win7 button states: flat → hover glow → pressed glass ──
                color: isActive ? Qt.rgba(1, 1, 1, 0.16)
                     : mouse.containsMouse ? Qt.rgba(1, 1, 1, 0.08)
                     : Qt.rgba(1, 1, 1, 0.0)
                border.color: isActive ? root.notes.barAccent
                            : mouse.containsMouse ? Qt.alpha(root.notes.barAccent, 0.45)
                            : "transparent"
                border.width: 1
                Behavior on color { ColorAnimation { duration: 120 } }

                // Pressed-glass gloss on the active button (its own sheen).
                Rectangle {
                    anchors.fill: parent
                    anchors.margins: 1
                    radius: 2
                    visible: button.isActive
                    gradient: Gradient {
                        GradientStop { position: 0.0;  color: Qt.rgba(1, 1, 1, 0.18) }
                        GradientStop { position: 0.5;  color: Qt.rgba(1, 1, 1, 0.02) }
                        GradientStop { position: 0.55; color: Qt.rgba(1, 1, 1, 0.00) }
                        GradientStop { position: 1.0;  color: Qt.rgba(1, 1, 1, 0.08) }
                    }
                }

                // ── App icon (minimized windows dim — the Win7 read) ───────
                Image {
                    id: icon
                    anchors.centerIn: parent
                    width: root.iconSize
                    height: root.iconSize
                    source: button.iconSrc
                    sourceSize.width: root.iconSize * 2
                    sourceSize.height: root.iconSize * 2
                    smooth: true
                    opacity: button.isMin ? 0.55 : 1.0
                    visible: status === Image.Ready
                }

                // Fallback: initial-letter cell when no themed icon resolves.
                Text {
                    anchors.centerIn: parent
                    visible: icon.status !== Image.Ready
                    text: button.appId.length > 0
                          ? button.appId.charAt(0).toUpperCase() : "?"
                    color: button.isActive ? root.notes.barAccent : root.notes.barFg
                    opacity: button.isMin ? 0.55 : 1.0
                    style: Text.Outline
                    styleColor: "#000000"
                    font.family: "monospace"
                    font.pixelSize: 15
                    font.bold: true
                }

                MouseArea {
                    id: mouse
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    acceptedButtons: Qt.LeftButton | Qt.MiddleButton
                    onClicked: function (ev) {
                        if (!button.modelData) return
                        if (ev.button === Qt.MiddleButton) {
                            button.modelData.close()
                            return
                        }
                        // Win7 toggle: active → minimize; else raise/focus.
                        if (button.isActive)
                            button.modelData.minimized = true
                        else
                            button.modelData.activate()
                    }
                }
            }
        }
    }
}
