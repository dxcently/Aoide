// AoideLauncher.qml — the app launcher (surface #3, "launcher").
//
// Quickshell-native application launcher — replaces rofi. A summoned overlay:
// hidden by default, it drops onto the OVERLAY layer with an EXCLUSIVE keyboard
// grab, filters the installed .desktop entries as you type, and launches the
// chosen one. Trigger is a Hyprland GLOBAL shortcut (aoide:launcher) the
// launcher registers in-process — the compositor binds SUPER+SPACE to it (see
// modules/facets/compositor: `bind = SUPER, SPACE, global, aoide:launcher`).
// Escape / a click on the scrim / launching an app dismisses it.
//
// ── Why GlobalShortcut, not the bridge ──────────────────────────────────────
// ShellBridge is OUTBOUND-only (QML → shellbridge socket), and the old
// `aoide shell launcher toggle` CLI verb the keybind used to call is an
// unimplemented stub (not in the aoide command schema). Rather than build a new
// inbound CLI+socket path, the launcher registers a Hyprland global shortcut and
// receives the keypress IN-PROCESS — the cleanest inbound trigger for a surface
// that lives in the shell. No new aoided verb, no SocketServer.
//
// ── Enumerate + launch (exec discipline) ────────────────────────────────────
// Apps come from Quickshell's built-in DesktopEntries (parsed XDG .desktop
// files); launching calls DesktopEntry.execute() directly. This is the same
// Quickshell-native-service idiom the rest of the shell already uses for side
// effects — WorkspaceRow calls Hyprland `ws.activate()`, AoideBar mutates the
// Pipewire sink, BatonGadget spawns via `Quickshell.execDetached(...)` — NOT a
// shell-out invented in QML. The alternative (route an app-launch verb through
// aoided per house rule #6) is a contract question flagged for khoa; today no
// such verb exists and adding one buys nothing over execute().
//
// Grammar: the panel is a Pantheon PANE (GadgetFrame — wireframe-depth stack +
// cream Aero glass + gloss + wireCyan outline + lowercase dotted callout), so it
// reads as the same system as the gadget dock. Colors ONLY from notes.

import QtQuick
import Quickshell
import Quickshell.Wayland
import Quickshell.Hyprland

PanelWindow {
    id: root

    // ── Note + bridge dependencies (injected by shell.qml) ─────────────────
    required property var notes
    required property var bridge

    // ── Visibility gate ────────────────────────────────────────────────────
    // `shown` drives everything: the surface only exists (visible) and only
    // grabs the keyboard while summoned; hidden, it is an inert zero-cost
    // layer. (Named `shown`, matching AoideAgentWidgets — the skeleton's older
    // `visible_` bool is retired now that the surface owns a real PanelWindow.)
    property bool shown: false

    function show()   { root.query = ""; root.selIndex = 0; root.shown = true }
    function hide()   { root.shown = false; root.query = "" }
    function toggle() { if (root.shown) root.hide(); else root.show() }

    // ── Layer-shell surface (Overlay, full-screen, transparent) ────────────
    // Overlay so the summon sits above every window and the dock. Full-screen
    // (all edges) to host the dim scrim + centred pane; non-exclusive (reserves
    // no space — it is transient). Keyboard focus is EXCLUSIVE while shown so
    // typing filters and Escape closes without the focused window stealing keys;
    // None while hidden so the surface never deadens input at rest.
    anchors { top: true; bottom: true; left: true; right: true }
    exclusiveZone: 0
    color: "transparent"
    visible: root.shown
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.namespace: "aoide-launcher"
    WlrLayershell.keyboardFocus: root.shown ? WlrKeyboardFocus.Exclusive
                                            : WlrKeyboardFocus.None

    // ── Trigger: Hyprland global shortcut (aoide:launcher) ─────────────────
    // Registered in-process; Hyprland binds it via `bind = …, global,
    // aoide:launcher` (compositor facet). Toggling here means one press summons,
    // the next dismisses — the launcher's whole reveal contract in one signal.
    GlobalShortcut {
        appid: "aoide"
        name: "launcher"
        description: "Summon the Aoide app launcher"
        onPressed: root.toggle()
    }

    // ── Search + selection state ────────────────────────────────────────────
    property string query: ""
    property int selIndex: 0

    // ── Filtered app list (substring match, prefix-ranked) ──────────────────
    // Source: Quickshell's DesktopEntries (parsed XDG .desktop). noDisplay
    // entries are dropped. A blank query lists everything alphabetically; a
    // query keeps entries whose name/genericName/comment CONTAINS it (case-
    // insensitive), ranked so name-PREFIX hits float to the top (the row you
    // meant is usually the one that starts with what you typed), then name
    // substring, then a metadata-only match, alphabetical within each tier.
    readonly property var apps: {
        var m = DesktopEntries.applications
        return (m && m.values) ? m.values : []
    }
    readonly property var filtered: {
        var q = ("" + root.query).trim().toLowerCase()
        var out = []
        for (var i = 0; i < root.apps.length; i++) {
            var e = root.apps[i]
            if (!e || e.noDisplay) continue
            var name = ("" + (e.name || "")).toLowerCase()
            var gen  = ("" + (e.genericName || "")).toLowerCase()
            var com  = ("" + (e.comment || "")).toLowerCase()
            var rank
            if (q.length === 0) {
                rank = 1
            } else if (name.indexOf(q) === 0) {
                rank = 0                       // name prefix — the strongest read
            } else if (name.indexOf(q) !== -1) {
                rank = 1                       // name substring
            } else if (gen.indexOf(q) !== -1 || com.indexOf(q) !== -1) {
                rank = 2                       // metadata-only match
            } else {
                continue                       // no match — drop
            }
            out.push({ "entry": e, "rank": rank, "name": name })
        }
        out.sort(function (a, b) {
            if (a.rank !== b.rank) return a.rank - b.rank
            return a.name < b.name ? -1 : (a.name > b.name ? 1 : 0)
        })
        var apps = []
        for (var j = 0; j < out.length; j++) apps.push(out[j].entry)
        return apps
    }

    // Keep the selection in range as the filter narrows/widens.
    onFilteredChanged: {
        if (root.selIndex >= root.filtered.length)
            root.selIndex = Math.max(0, root.filtered.length - 1)
    }
    onQueryChanged: root.selIndex = 0

    // ── Navigation helpers ──────────────────────────────────────────────────
    function move(delta) {
        var n = root.filtered.length
        if (n === 0) return
        var i = root.selIndex + delta
        if (i < 0) i = 0
        if (i > n - 1) i = n - 1
        root.selIndex = i
        resultsList.positionViewAtIndex(i, ListView.Contain)
    }
    function launchSelected() {
        if (root.selIndex < 0 || root.selIndex >= root.filtered.length) return
        var e = root.filtered[root.selIndex]
        if (e && e.execute) e.execute()
        root.hide()
    }

    // Grab focus for the search field the moment the surface is summoned (the
    // layer only takes keyboard focus once mapped, so re-force on each show).
    onShownChanged: if (root.shown) searchInput.forceActiveFocus()

    // ══ Dim scrim — a click anywhere outside the pane dismisses (focus-loss
    // read). Soft umber veil, keyed off the song's ink so the darken belongs to
    // the palette rather than a neutral black. ═══════════════════════════════
    MouseArea {
        anchors.fill: parent
        onClicked: root.hide()
        Rectangle {
            anchors.fill: parent
            color: Qt.rgba(Qt.color(root.notes.paletteFg).r,
                           Qt.color(root.notes.paletteFg).g,
                           Qt.color(root.notes.paletteFg).b, 0.28)
        }
    }

    // ══ THE SUMMON PANE ══════════════════════════════════════════════════════
    // A Pantheon pane (GadgetFrame) centred on the screen — the vanishing point
    // itself, so its depth stack leans the default down-right. The search field
    // and the results run live in the frame body. A MouseArea over the frame
    // swallows clicks so they don't fall through to the scrim's dismiss.
    GadgetFrame {
        id: pane
        anchors.centerIn: parent
        width: 560
        notes: root.notes
        title: "launcher.summon"
        // A modal summon reads MORE solid than an ambient dock gadget (it sits
        // over arbitrary busy windows, not the calm wallpaper), so the glass is
        // pushed up from the dock's 0.72 — legible even where the compositor
        // blur behind aoide-launcher isn't active yet (pre-switch). The
        // compositor facet's blur/hyprglass rules for the aoide-launcher
        // namespace frost it fully once landed.
        glassOpacity: 0.9

        MouseArea { anchors.fill: parent; onClicked: {} }   // swallow pane clicks

        Column {
            width: parent.width
            spacing: 8

            // ── Search field — a plain TextInput with a manual placeholder and
            // a leading ♪ prompt (plain TextInput has no placeholderText; the
            // AoideLockscreen / stub idiom). Cream glass over the pane glass.
            Rectangle {
                width: parent.width
                height: 34
                radius: 0
                color: Qt.rgba(Qt.color(root.notes.paletteBg).r,
                               Qt.color(root.notes.paletteBg).g,
                               Qt.color(root.notes.paletteBg).b, 0.55)
                border.color: root.notes.wireCyan
                border.width: 1

                Text {
                    id: prompt
                    anchors { left: parent.left; leftMargin: 9; verticalCenter: parent.verticalCenter }
                    text: "♪"
                    color: root.notes.paletteAccent
                    font.family: "monospace"
                    font.pixelSize: 14
                    font.bold: true
                }

                TextInput {
                    id: searchInput
                    anchors {
                        left: prompt.right; leftMargin: 8
                        right: parent.right; rightMargin: 9
                        verticalCenter: parent.verticalCenter
                    }
                    focus: true
                    color: root.notes.paletteFg
                    font.family: "monospace"
                    font.pixelSize: 14
                    selectionColor: root.notes.paletteAccent
                    selectedTextColor: root.notes.paletteBg
                    clip: true
                    text: root.query
                    onTextChanged: root.query = text

                    // Placeholder overlay (no native placeholderText on TextInput).
                    Text {
                        anchors { left: parent.left; verticalCenter: parent.verticalCenter }
                        text: "summon an app…"
                        color: root.notes.paletteFg
                        opacity: 0.45
                        font.family: "monospace"
                        font.pixelSize: 14
                        visible: searchInput.text.length === 0
                    }

                    // Keyboard: type filters; Up/Down + Ctrl+K/J move the
                    // selection; Enter launches; Escape dismisses. Handled on the
                    // focused field so no separate FocusScope is needed.
                    Keys.onPressed: function (event) {
                        if (event.key === Qt.Key_Down
                                || (event.key === Qt.Key_J && (event.modifiers & Qt.ControlModifier))) {
                            root.move(1); event.accepted = true
                        } else if (event.key === Qt.Key_Up
                                || (event.key === Qt.Key_K && (event.modifiers & Qt.ControlModifier))) {
                            root.move(-1); event.accepted = true
                        } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
                            root.launchSelected(); event.accepted = true
                        } else if (event.key === Qt.Key_Escape) {
                            root.hide(); event.accepted = true
                        }
                    }
                }
            }

            // ── Results run ──────────────────────────────────────────────────
            // The filtered entries, one row each. Fixed viewport height so the
            // pane keeps a stable size as the list narrows; the ListView scrolls
            // and keeps the selection in view (see move()).
            ListView {
                id: resultsList
                width: parent.width
                height: 360
                clip: true
                model: root.filtered
                currentIndex: root.selIndex
                boundsBehavior: Flickable.StopAtBounds

                // Empty state — the pane's own little empty-title note.
                Text {
                    anchors.centerIn: parent
                    visible: root.filtered.length === 0
                    text: "no app by that name ♪(´ε｀ )"
                    color: root.notes.paletteFg
                    opacity: 0.5
                    font.family: "monospace"
                    font.pixelSize: 13
                }

                delegate: Rectangle {
                    id: row
                    required property var modelData
                    required property int index
                    readonly property bool isSel: root.selIndex === row.index

                    width: resultsList.width
                    height: 42
                    radius: 0
                    // Selected row is the CURRENT element — boxed in the song's
                    // accent over a faint accent wash (the "active workspace"
                    // reading), NOT the reserved hot green (that blaze belongs to
                    // live agent sessions alone). Resting rows are transparent.
                    color: row.isSel
                           ? Qt.rgba(Qt.color(root.notes.paletteAccent).r,
                                     Qt.color(root.notes.paletteAccent).g,
                                     Qt.color(root.notes.paletteAccent).b, 0.14)
                           : "transparent"
                    border.color: root.notes.paletteAccent
                    border.width: row.isSel ? 1 : 0

                    Row {
                        anchors {
                            left: parent.left; leftMargin: 10
                            right: parent.right; rightMargin: 10
                            verticalCenter: parent.verticalCenter
                        }
                        spacing: 10

                        // App icon — Quickshell resolves the .desktop icon name
                        // to a themed path; a generic exec glyph backs a miss.
                        Image {
                            anchors.verticalCenter: parent.verticalCenter
                            width: 26; height: 26
                            sourceSize.width: 26; sourceSize.height: 26
                            fillMode: Image.PreserveAspectFit
                            source: (row.modelData && row.modelData.icon)
                                    ? Quickshell.iconPath(row.modelData.icon, "application-x-executable")
                                    : Quickshell.iconPath("application-x-executable")
                        }

                        // Name + a dim lowercase callout (genericName/comment) —
                        // the refs' label-on-a-pane read, at row scale.
                        Column {
                            anchors.verticalCenter: parent.verticalCenter
                            spacing: 1
                            Text {
                                text: (row.modelData && row.modelData.name) ? row.modelData.name : ""
                                color: root.notes.paletteFg
                                opacity: row.isSel ? 1.0 : 0.9
                                font.family: "monospace"
                                font.pixelSize: 14
                                font.bold: row.isSel
                            }
                            Text {
                                readonly property string sub:
                                    (row.modelData && row.modelData.genericName) ? row.modelData.genericName
                                    : ((row.modelData && row.modelData.comment) ? row.modelData.comment : "")
                                visible: sub.length > 0
                                text: sub
                                color: root.notes.paletteFg
                                opacity: 0.5
                                font.family: "monospace"
                                font.pixelSize: 11
                                elide: Text.ElideRight
                                width: resultsList.width - 66
                            }
                        }
                    }

                    // Mouse: hover selects, click launches (keeps focus on the
                    // field so typing continues after a click).
                    MouseArea {
                        anchors.fill: parent
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onEntered: root.selIndex = row.index
                        onClicked: {
                            root.selIndex = row.index
                            root.launchSelected()
                        }
                    }
                }
            }
        }
    }
}
