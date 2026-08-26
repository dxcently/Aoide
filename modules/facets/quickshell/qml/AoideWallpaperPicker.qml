// AoideWallpaperPicker.qml — the wallpaper switcher (surface: "wallpaper").
//
// Quickshell-native wallpaper picker over the shared cover library
// song/covers/. A summoned overlay: hidden by default, it drops onto the
// OVERLAY layer with an EXCLUSIVE keyboard grab, shows a GridView of cover
// thumbnails, and on pick switches the LIVE wallpaper. Trigger is a Hyprland
// GLOBAL shortcut (aoide:wallpaper) registered in-process — the compositor
// binds a combo to it (see modules/dendrites/hyprland.nix:
// `bind = SUPER, W, global, aoide:wallpaper`). Escape / a scrim click / a pick
// dismisses it.
//
// ── The write seam ──────────────────────────────────────────────────────────
// QML has no file-write primitive and the shellbridge socket only speaks
// `focuswindow`, so switching the wallpaper cannot write stage/cover.json from
// here. Instead a pick shells out through the ONE CLI command built for this —
// `lyra cover set <path>` (Quickshell.execDetached, the same exec idiom
// ConductorGadget uses) — which atomic-writes stage/cover.json. `cover` left
// core's registry at P-A5 of the binary-split workstream and lives only in
// `lyra` now (bare name resolves via PATH: modules/nucleus/packages.nix puts
// lyra's own droppable output, pkgs.aoide.rice, on systemPackages whenever
// aoide.lyra.enable is on — P-A8 — which defaults to true here since this
// widget only exists under the quickshell facet).
// AoideWallpaper.qml
// FileView-watches that file and hot-swaps the live wallpaper. No new socket,
// no QML file write.
//
// ── Enumerate covers ────────────────────────────────────────────────────────
// The covers dir is listed with a Quickshell.Io Process (`ls -1`) whose stdout
// a StdioCollector gathers; we keep the recognised image extensions and build
// absolute file:// paths. Re-lists on every summon so a freshly dropped cover
// shows up. (Quickshell 0.3 ships no folder-model primitive; a Process is the
// house idiom for a one-shot side read.)
//
// Colors ONLY from livery roles (paletteBg/paletteFg/paletteAccent);
// radius:0 throughout. Deliberately plain: a flat pane, a thumbnail grid,
// and a one-pixel selection border. No decorative styling.

import QtQuick
import Quickshell
import Quickshell.Wayland
import Quickshell.Hyprland
import Quickshell.Io

PanelWindow {
    id: root

    // ── Note dependency (injected by shell.qml) ────────────────────────────
    required property var livery

    // ── Visibility gate (mirrors AoideLauncher) ────────────────────────────
    property bool shown: false

    function show()   { root.selIndex = 0; root.shown = true; lister.reload() }
    function hide()   { root.shown = false }
    function toggle() { if (root.shown) root.hide(); else root.show() }

    // ── Layer-shell surface (Overlay, full-screen, transparent) ────────────
    anchors { top: true; bottom: true; left: true; right: true }
    exclusiveZone: 0
    color: "transparent"
    visible: root.shown
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "aoide-wallpaper"
    WlrLayershell.keyboardFocus: root.shown ? WlrKeyboardFocus.Exclusive
                                            : WlrKeyboardFocus.None

    // ── Trigger: Hyprland global shortcut (aoide:wallpaper) ────────────────
    GlobalShortcut {
        appid: "aoide"
        name: "wallpaper"
        description: "Summon the Aoide wallpaper switcher"
        onPressed: root.toggle()
    }

    // ── Cover enumeration ───────────────────────────────────────────────────
    readonly property string coversDir: Quickshell.env("HOME") + "/Aoide/song/covers"
    readonly property var coverExts: ["webp", "png", "jpg", "jpeg"]
    property var covers: []           // [{ name, path }]
    property int selIndex: 0

    // One-shot directory read. `ls -1` gives bare filenames; we filter by
    // extension and join to absolute paths. StdioCollector gathers the whole
    // stream, then we parse once on streamFinished.
    Process {
        id: lister
        command: ["ls", "-1", root.coversDir]
        stdout: StdioCollector {
            id: coverOut
            onStreamFinished: {
                var lines = ("" + coverOut.text).split("\n")
                var out = []
                for (var i = 0; i < lines.length; i++) {
                    var name = ("" + lines[i]).trim()
                    if (name.length === 0) continue
                    var dot = name.lastIndexOf(".")
                    if (dot < 0) continue
                    var ext = name.substring(dot + 1).toLowerCase()
                    if (root.coverExts.indexOf(ext) === -1) continue
                    out.push({ "name": name, "path": root.coversDir + "/" + name })
                }
                out.sort(function (a, b) {
                    return a.name < b.name ? -1 : (a.name > b.name ? 1 : 0)
                })
                root.covers = out
                if (root.selIndex >= out.length)
                    root.selIndex = Math.max(0, out.length - 1)
            }
        }
        function reload() { lister.running = false; lister.running = true }
    }

    Component.onCompleted: lister.reload()

    // ── Navigation + apply ──────────────────────────────────────────────────
    function move(delta) {
        var n = root.covers.length
        if (n === 0) return
        var i = root.selIndex + delta
        if (i < 0) i = 0
        if (i > n - 1) i = n - 1
        root.selIndex = i
        grid.positionViewAtIndex(i, GridView.Contain)
    }
    function applySelected() {
        if (root.selIndex < 0 || root.selIndex >= root.covers.length) return
        var c = root.covers[root.selIndex]
        if (c && c.path)
            Quickshell.execDetached(["lyra", "cover", "set", c.path])
        root.hide()
    }
    function applyAt(index) {
        if (index < 0 || index >= root.covers.length) return
        root.selIndex = index
        root.applySelected()
    }

    // Grab focus for the key handler the moment the surface is summoned.
    onShownChanged: if (root.shown) keyCatch.forceActiveFocus()

    // ══ Dim scrim — a click anywhere outside the pane dismisses. Soft umber
    // veil keyed off the song's ink (matches AoideLauncher). ══════════════════
    MouseArea {
        anchors.fill: parent
        onClicked: root.hide()
        Rectangle {
            anchors.fill: parent
            color: Qt.rgba(Qt.color(root.livery.paletteFg).r,
                           Qt.color(root.livery.paletteFg).g,
                           Qt.color(root.livery.paletteFg).b, 0.28)
        }
    }

    // ══ THE SUMMON PANE — a plain flat rectangle, no frame, no title. ════════
    Rectangle {
        id: pane
        anchors.centerIn: parent
        width: 620
        height: 404
        radius: 0
        color: root.livery.paletteBg
        border.color: root.livery.paletteFg
        border.width: 1

        MouseArea { anchors.fill: parent; onClicked: {} }   // swallow pane clicks

        // Keyboard: arrows / hjkl move; Enter applies; Escape dismisses. Held on
        // an invisible focus item so no TextInput is needed (there's no query).
        Item {
            id: keyCatch
            anchors.fill: parent
            focus: true
            Keys.onPressed: function (event) {
                if (event.key === Qt.Key_Right
                        || (event.key === Qt.Key_L && (event.modifiers & Qt.ControlModifier))
                        || event.key === Qt.Key_L) {
                    root.move(1); event.accepted = true
                } else if (event.key === Qt.Key_Left
                        || event.key === Qt.Key_H) {
                    root.move(-1); event.accepted = true
                } else if (event.key === Qt.Key_Down
                        || event.key === Qt.Key_J) {
                    root.move(grid.columns); event.accepted = true
                } else if (event.key === Qt.Key_Up
                        || event.key === Qt.Key_K) {
                    root.move(-grid.columns); event.accepted = true
                } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                    root.applySelected(); event.accepted = true
                } else if (event.key === Qt.Key_Escape) {
                    root.hide(); event.accepted = true
                }
            }
        }

        Column {
            anchors.centerIn: parent
            width: parent.width - 24

            // ── Cover grid ────────────────────────────────────────────────────
            GridView {
                id: grid
                width: parent.width
                height: 380
                clip: true
                readonly property int columns: 3
                readonly property int gutter: 8
                cellWidth: Math.floor(width / columns)
                cellHeight: Math.round(cellWidth * 0.62)
                model: root.covers
                currentIndex: root.selIndex
                boundsBehavior: Flickable.StopAtBounds

                // Empty state.
                Text {
                    anchors.centerIn: parent
                    visible: root.covers.length === 0
                    text: "no covers in song/covers/"
                    color: root.livery.paletteFg
                    opacity: 0.5
                    font.family: "monospace"
                    font.pixelSize: 13
                }

                delegate: Item {
                    id: cell
                    required property var modelData
                    required property int index
                    readonly property bool isSel: root.selIndex === cell.index
                    width: grid.cellWidth
                    height: grid.cellHeight

                    // Plain cell: thumbnail, a one-pixel accent border when
                    // selected, nothing otherwise.
                    Rectangle {
                        anchors.fill: parent
                        anchors.margins: grid.gutter / 2
                        radius: 0
                        color: "transparent"
                        border.color: cell.isSel ? root.livery.paletteAccent
                                                 : "transparent"
                        border.width: 1

                        Image {
                            id: thumb
                            anchors.fill: parent
                            anchors.margins: 2
                            source: (cell.modelData && cell.modelData.path)
                                    ? "file://" + cell.modelData.path : ""
                            sourceSize.width: 320
                            sourceSize.height: 200
                            fillMode: Image.PreserveAspectCrop
                            clip: true
                            asynchronous: true
                            smooth: true
                        }

                        // Filename along the bottom, plain solid strip.
                        Rectangle {
                            anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
                            height: 18
                            color: root.livery.paletteBg
                            Text {
                                anchors { left: parent.left; leftMargin: 6; right: parent.right; rightMargin: 6; verticalCenter: parent.verticalCenter }
                                text: (cell.modelData && cell.modelData.name) ? cell.modelData.name : ""
                                color: root.livery.paletteFg
                                font.family: "monospace"
                                font.pixelSize: 11
                                elide: Text.ElideRight
                            }
                        }

                        MouseArea {
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onEntered: root.selIndex = cell.index
                            onClicked: root.applyAt(cell.index)
                        }
                    }
                }
            }
        }
    }
}
