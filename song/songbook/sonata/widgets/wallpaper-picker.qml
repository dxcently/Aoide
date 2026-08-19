// wallpaper-picker.qml — sonata's "wallpaper-picker" slot: the wallpaper
// switcher (surface: "wallpaper").
//
// Ported from the facet's AoideWallpaperPicker.qml (per-song widget-slot
// expansion, CONTRACTS.md §5) — ownership moves from the facet to sonata's
// score; the facet original stays in place until a later phase (Phase 6)
// retires it, so this file and AoideWallpaperPicker.qml are momentarily
// twins. This slot is hosted by `SurfaceSlot`, not `WidgetSlot` — the root
// below is a `PanelWindow`, not an `Item`, so it owns its own layer,
// namespace and keyboard focus, the same contract powermenu.qml, launcher.qml
// and dock.qml already carry (see slots.md). That SurfaceSlot host wiring
// itself lands with Phase 6, not here.
//
// ── Why GlobalShortcut stays OUT of this file ───────────────────────────────
// The facet's AoideWallpaperPicker.qml registers its own `aoide:wallpaper`
// Hyprland global shortcut in-process — the launcher's in-process inbound
// idiom (ShellBridge is OUTBOUND-only, no inbound CLI verb to toggle a
// surface). That keybind stays a FACET contract on purpose, same rationale
// as dock.qml's own GlobalShortcut carve-out: Phase 6 keeps it in shell.qml,
// dispatching into this slot, so a future song's wallpaper-picker body can't
// silently forget to bind it. This file instead exposes the unconditional
// `show()`/`hide()`/`toggle()` trio on its own root (below) — the same shape
// powermenu.qml, launcher.qml and dock.qml already expose — for the host to
// call once SurfaceSlot hands back `.item`.
//
// Quickshell-native wallpaper picker over the shared cover library
// song/covers/. A summoned overlay: hidden by default, it drops onto the
// OVERLAY layer with an EXCLUSIVE keyboard grab, shows a GridView of cover
// thumbnails, and on pick switches the LIVE wallpaper. Escape / a scrim
// click / a pick dismisses it.
//
// ── The write seam ──────────────────────────────────────────────────────────
// QML has no file-write primitive and the shellbridge socket only speaks
// `focuswindow`, so switching the wallpaper cannot write stage/cover.json from
// here. Instead a pick shells out through the ONE CLI verb built for this —
// `aoide cover set <path>` (Quickshell.execDetached, the same exec idiom
// ConductorGadget uses) — which atomic-writes stage/cover.json. wallpaper.qml
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
// Colors ONLY from livery roles (paletteBg/paletteFg/paletteAccent/wireCyan);
// radius:0 throughout. Greek grammar styling is a LATER pass — this is the
// functional, palette-correct build.

import QtQuick
import Quickshell
import Quickshell.Wayland
import Quickshell.Io

PanelWindow {
    id: root

    // ── Note + bridge dependencies (injected as SurfaceSlot extras by the
    // host, Phase 6) ─────────────────────────────────────────────────────────
    required property var livery
    required property var bridge

    // ── Visibility gate (mirrors launcher.qml) ──────────────────────────────
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
    // NOT "aoide-wallpaper" — that string belongs to the Background surface
    // the desktop cover is painted on (shell.qml), and the facet original
    // this file was ported from shared it. Two surfaces under one namespace
    // are indistinguishable to the compositor's layerrules, so glassing this
    // summon pane the way aoide-launcher/aoide-powermenu are glassed would
    // have blurred the desktop background along with it. Renamed here, while
    // the slot is still unanchored and the string is nobody's contract yet:
    // once a host wires this slot the namespace becomes documented contract
    // (slots.md, "Window-owning slot namespaces") and is no longer a song
    // widget's to rename.
    WlrLayershell.namespace: "aoide-wallpaper-picker"
    WlrLayershell.keyboardFocus: root.shown ? WlrKeyboardFocus.Exclusive
                                            : WlrKeyboardFocus.None

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
            Quickshell.execDetached(["aoide", "cover", "set", c.path])
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
    // veil keyed off the song's ink (matches launcher.qml). ═════════════════
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

    // ══ THE SUMMON PANE ══════════════════════════════════════════════════════
    GadgetFrame {
        id: pane
        anchors.centerIn: parent
        width: 620
        livery: root.livery
        title: "wallpaper.summon"
        glassOpacity: 0.9

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
            width: parent.width
            spacing: 8

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
                    text: "no covers in song/covers/ ♪(´ε｀ )"
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

                    // Selected thumb gets the launcher's accent-box treatment.
                    Rectangle {
                        anchors.fill: parent
                        anchors.margins: grid.gutter / 2
                        radius: 0
                        color: Qt.rgba(Qt.color(root.livery.paletteBg).r,
                                       Qt.color(root.livery.paletteBg).g,
                                       Qt.color(root.livery.paletteBg).b, 0.35)
                        border.color: cell.isSel ? root.livery.paletteAccent
                                                 : root.livery.wireCyan
                        border.width: cell.isSel ? 2 : 1

                        Image {
                            id: thumb
                            anchors.fill: parent
                            anchors.margins: cell.isSel ? 3 : 2
                            source: (cell.modelData && cell.modelData.path)
                                    ? "file://" + cell.modelData.path : ""
                            sourceSize.width: 320
                            sourceSize.height: 200
                            fillMode: Image.PreserveAspectCrop
                            clip: true
                            asynchronous: true
                            smooth: true
                        }

                        // Selected wash over the whole cell (the "current" read).
                        Rectangle {
                            anchors.fill: parent
                            visible: cell.isSel
                            color: Qt.rgba(Qt.color(root.livery.paletteAccent).r,
                                           Qt.color(root.livery.paletteAccent).g,
                                           Qt.color(root.livery.paletteAccent).b, 0.14)
                        }

                        // Filename callout along the bottom.
                        Rectangle {
                            anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
                            height: 18
                            color: Qt.rgba(Qt.color(root.livery.paletteBg).r,
                                           Qt.color(root.livery.paletteBg).g,
                                           Qt.color(root.livery.paletteBg).b, 0.62)
                            Text {
                                anchors { left: parent.left; leftMargin: 6; right: parent.right; rightMargin: 6; verticalCenter: parent.verticalCenter }
                                text: (cell.modelData && cell.modelData.name) ? cell.modelData.name : ""
                                color: root.livery.paletteFg
                                opacity: cell.isSel ? 1.0 : 0.8
                                font.family: "monospace"
                                font.pixelSize: 11
                                font.bold: cell.isSel
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
