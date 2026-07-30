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
// ── Grammar: THE PROPYLAEA (the glass gate) ─────────────────────────────────
// The four dock temples are OPAQUE marble steles. The launcher is the exception
// — it is the ENTRANCE, the propylaea, the gate you pass through to summon an
// app, so it stays a FROSTED-GLASS temple: the compositor still blurs + hyprglass-
// refracts the `aoide-launcher` namespace behind it, and the body is drawn as the
// song's paletteBg at a TRANSLUCENT alpha so that blur frosts THROUGH the pane.
// It still wears the full pantheon chrome — hard 2px plum border, inset Attic-gold
// keyline, radius 0, a ⌘/𝄞 carved-serif inscription (its own gateway signature,
// distinct from the dock's Doric orders), a Greek-key meander rule, a ┤ … ├ TUI
// frame around the search line, the ♪ prompt idiom, and kaomoji life on the empty
// stage — but as GLASS, not marble. The selected row is the one laurel: a
// paletteHot ♪ note + spine over an Attic-gold accent box. Colours from `notes`.

import QtQuick
import QtQuick.Effects
import Quickshell
import Quickshell.Wayland
import Quickshell.Hyprland

PanelWindow {
    id: root

    // ── Note + bridge dependencies (injected by shell.qml) ─────────────────
    required property var notes
    required property var bridge

    // ── Type voices (the Conductor family) ─────────────────────────────────
    readonly property string faceSerif: "Noto Serif"                // carved marble
    readonly property string faceMono:  "JetBrainsMono Nerd Font"   // the terminal
    readonly property string faceMusic: "Noto Music"                // notation

    // alpha on a role string
    function withA(cstr, a) {
        var c = Qt.color(cstr)
        return Qt.rgba(c.r, c.g, c.b, a)
    }

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
            color: root.withA(root.notes.paletteFg, 0.28)
        }
    }

    // ══ THE PROPYLAEA — the glass gate ═════════════════════════════════════════
    // A translucent glass temple centred on the screen: hard 2px plum border, an
    // inset Attic-gold keyline, radius 0 — a defined pantheon pane — but the body
    // is paletteBg at a glassy alpha so the compositor blur/hyprglass frosts
    // THROUGH it. A MouseArea over the pane swallows clicks so they don't fall
    // through to the scrim's dismiss.
    Item {
        id: pane
        anchors.centerIn: parent
        width: 560
        height: paneCol.implicitHeight + 26

        MouseArea { anchors.fill: parent; onClicked: {} }   // swallow pane clicks

        // ── Summoning halo — a single soft WHITE glow, genuinely BLURRED ─────
        // One white ring source (transparent-CENTRED so it can never wash the
        // body) pushed through a MultiEffect Gaussian blur → a soft radiant aura
        // that hugs the gate when it's up. Declared FIRST → drawn behind the
        // shadow, glass, border and body, so the bloom sits AROUND the pane and
        // never touches the search text or rows. The source ring itself is
        // hidden (visible:false) so only its blurred bloom shows — no hard ring.
        // White literal is sanctioned here: it is a glow, not palette chrome.
        Rectangle {
            id: glowSource
            anchors.fill: glass
            anchors.margins: -5
            visible: false
            radius: 0
            color: "transparent"
            border.color: "white"
            border.width: 6
        }
        MultiEffect {
            source: glowSource
            anchors.fill: glowSource
            blurEnabled: true
            blur: 1.0
            blurMax: 48
            autoPaddingEnabled: true
            opacity: 0.6
        }

        // ── Cast shadow — lifts the gate off any busy window behind it ───────
        Rectangle {
            anchors.fill: glass
            anchors.leftMargin: 4; anchors.topMargin: 5
            anchors.rightMargin: -4; anchors.bottomMargin: -5
            radius: 0
            color: root.withA(root.notes.paletteFg, 0.22)
        }

        // ── The glass body ──────────────────────────────────────────────────
        // FROSTED, not marble: the fill is paletteBg at a translucent alpha so
        // the compositor blur behind aoide-launcher refracts through the pane.
        // Pushed a touch higher than the dock's 0.72 — this summon floats over
        // arbitrary busy windows and must stay legible even before the switch
        // that turns the namespace blur on. The 2px plum border + gold keyline
        // are drawn at FULL alpha over the glass so the frame reads crisp while
        // the field behind it stays see-through.
        property real glassOpacity: 0.82

        Rectangle {
            id: glass
            anchors.fill: parent
            radius: 0
            color: root.withA(root.notes.paletteBg, pane.glassOpacity)
            border.color: root.notes.paletteFg      // hard 2px plum keyframe
            border.width: 2

            // Gloss — the Aero sheen (bright top half, hard midline stop).
            Rectangle {
                anchors.fill: parent
                anchors.margins: 2
                radius: 0
                gradient: Gradient {
                    GradientStop { position: 0.0;  color: Qt.rgba(1, 1, 1, 0.14) }
                    GradientStop { position: 0.42; color: Qt.rgba(1, 1, 1, 0.04) }
                    GradientStop { position: 0.5;  color: Qt.rgba(1, 1, 1, 0.00) }
                    GradientStop { position: 1.0;  color: Qt.rgba(1, 1, 1, 0.04) }
                }
            }

            // Inset Attic-gold keyline.
            Rectangle {
                anchors.fill: parent; anchors.margins: 4
                radius: 0; color: "transparent"
                border.color: root.notes.paletteAccent; border.width: 1
            }
        }

        // ── The gate's contents ─────────────────────────────────────────────
        Column {
            id: paneCol
            anchors.top: parent.top
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.margins: 13
            spacing: 8

            // ── ENTABLATURE: ⌘ key · SUMMON inscription · πύλη · [ spc ] tag ──
            // The propylaea's own signature — a ⌘ summon-key glyph in place of the
            // dock's clef, a carved-serif SUMMON, the Greek word πύλη ("gate")
            // whispered beside it, and a mono keycap tag at the tail.
            Item {
                id: head
                width: parent.width
                height: 40

                Rectangle {                           // deeper glass band
                    anchors.fill: parent; anchors.bottomMargin: 5
                    color: root.withA(root.notes.paletteFg, 0.05)
                }
                Text {
                    id: keyGlyph
                    anchors.left: parent.left
                    anchors.verticalCenter: parent.verticalCenter
                    anchors.verticalCenterOffset: -2
                    text: "⌘"
                    font.family: root.faceSerif; font.pixelSize: 26
                    color: root.notes.paletteAccent
                }
                Text {
                    id: summonTitle
                    anchors.left: keyGlyph.right; anchors.leftMargin: 10
                    anchors.verticalCenter: parent.verticalCenter
                    text: "SUMMON"
                    font.family: root.faceSerif; font.pixelSize: 19
                    font.weight: Font.DemiBold; font.letterSpacing: 4
                    color: root.notes.paletteFg
                }
                Text {                                // the Greek gate whispered
                    anchors.left: summonTitle.right; anchors.leftMargin: 9
                    anchors.baseline: summonTitle.baseline
                    text: "πύλη"
                    font.family: root.faceSerif; font.italic: true
                    font.pixelSize: 13
                    color: root.withA(root.notes.paletteFg, 0.55)
                }
                Text {                                // keycap tag
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    text: "[ ⌘ spc ]"
                    font.family: root.faceMono; font.pixelSize: 11
                    color: root.withA(root.notes.wireCyan, 0.95)
                }
            }

            // ── Greek-key meander — the fret rule ruling off the entablature ──
            Canvas {
                id: meander
                width: parent.width; height: 13
                onWidthChanged: requestPaint()
                onPaint: {
                    var ctx = getContext("2d")
                    ctx.reset()
                    ctx.clearRect(0, 0, width, height)
                    ctx.strokeStyle = root.notes.paletteAccent
                    ctx.lineWidth = 1.4
                    var u = height / 5
                    var top = u * 0.5, bot = height - u * 0.5, period = u * 6
                    ctx.beginPath()
                    ctx.moveTo(0, top); ctx.lineTo(width, top)
                    ctx.moveTo(0, bot); ctx.lineTo(width, bot)
                    ctx.stroke()
                    ctx.beginPath()
                    for (var x = 0; x < width; x += period) {
                        ctx.moveTo(x + u,     top)
                        ctx.lineTo(x + u,     bot - u)
                        ctx.lineTo(x + u * 4, bot - u)
                        ctx.lineTo(x + u * 4, top + u)
                        ctx.lineTo(x + u * 2, top + u)
                        ctx.lineTo(x + u * 2, bot - u * 2)
                        ctx.lineTo(x + u * 3, bot - u * 2)
                    }
                    ctx.stroke()
                }
            }

            // ── The search line, framed like a TUI cell: ┤ ♪ … ├ ─────────────
            // The ┤ … ├ brackets frame the field; the ♪ prompt sits inside, the
            // AoideLockscreen idiom (plain TextInput has no placeholderText, so a
            // manual overlay backs it). The field's own glass sits over the pane
            // glass — a wireCyan/gold framed cell.
            Item {
                id: searchFrame
                width: parent.width
                height: 36

                Text {
                    id: sfL
                    anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter
                    text: "┤"
                    font.family: root.faceMono; font.pixelSize: 22
                    color: root.withA(root.notes.paletteAccent, 0.95)
                }
                Text {
                    id: sfR
                    anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                    text: "├"
                    font.family: root.faceMono; font.pixelSize: 22
                    color: root.withA(root.notes.paletteAccent, 0.95)
                }

                Rectangle {
                    anchors.left: sfL.right; anchors.leftMargin: 4
                    anchors.right: sfR.left; anchors.rightMargin: 4
                    anchors.verticalCenter: parent.verticalCenter
                    height: 34
                    radius: 0
                    color: root.withA(root.notes.paletteBg, 0.45)
                    border.color: root.notes.wireCyan
                    border.width: 1

                    Text {
                        id: prompt
                        anchors { left: parent.left; leftMargin: 9; verticalCenter: parent.verticalCenter }
                        text: "♪"
                        color: root.notes.paletteAccent
                        font.family: root.faceMusic
                        font.pixelSize: 15
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
                        font.family: root.faceMono
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
                            font.family: root.faceMono
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

                // Empty state — the gate's own little kaomoji shrug.
                Text {
                    anchors.centerIn: parent
                    visible: root.filtered.length === 0
                    text: "no app by that name ♪(´ε｀ )"
                    color: root.notes.paletteFg
                    opacity: 0.5
                    font.family: root.faceMono
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
                    // The selected row is the ONE laurel: an Attic-gold accent box
                    // over a faint accent wash (the "current element" read), with a
                    // paletteHot ♪ note + spine as the single standout. Resting rows
                    // are transparent.
                    color: row.isSel
                           ? root.withA(root.notes.paletteAccent, 0.14)
                           : "transparent"
                    border.color: root.notes.paletteAccent
                    border.width: row.isSel ? 1 : 0

                    // laurel spine — the one standout, on the selected row only
                    Rectangle {
                        anchors.left: parent.left; anchors.top: parent.top
                        anchors.bottom: parent.bottom
                        width: 3; color: root.notes.paletteHot; visible: row.isSel
                    }

                    Row {
                        anchors {
                            left: parent.left; leftMargin: 10
                            right: parent.right; rightMargin: 10
                            verticalCenter: parent.verticalCenter
                        }
                        spacing: 10

                        // Note glyph — the selected row sings a laurel ♪; resting
                        // rows show a dim rest-dot. (Music state contract, at rest.)
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            width: 14
                            horizontalAlignment: Text.AlignHCenter
                            text: row.isSel ? "♪" : "·"
                            font.family: root.faceMusic
                            font.pixelSize: row.isSel ? 17 : 15
                            color: row.isSel ? root.notes.paletteHot
                                             : root.withA(root.notes.paletteFg, 0.35)
                        }

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
                                font.family: root.faceSerif
                                font.pixelSize: 14
                                font.weight: row.isSel ? Font.Bold : Font.Medium
                            }
                            Text {
                                readonly property string sub:
                                    (row.modelData && row.modelData.genericName) ? row.modelData.genericName
                                    : ((row.modelData && row.modelData.comment) ? row.modelData.comment : "")
                                visible: sub.length > 0
                                text: sub
                                color: root.withA(root.notes.holoBlue, 0.95)
                                font.family: root.faceMono
                                font.pixelSize: 11
                                elide: Text.ElideRight
                                width: resultsList.width - 80
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

            // ── box-drawing footer: └─┤ N apps ├──── 𝄂 ┘ ─────────────────────
            // The score's closing course — a tally of the filtered set, a final
            // barline 𝄂, and the corner that shuts the frame.
            Item {
                id: footer
                width: parent.width
                height: 18

                Text {
                    id: ffL
                    anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter
                    text: "└─┤ " + root.filtered.length + " app" + (root.filtered.length === 1 ? "" : "s") + " ├"
                    font.family: root.faceMono; font.pixelSize: 11
                    color: root.withA(root.notes.paletteFg, 0.8)
                }
                Text {
                    id: ffCorner
                    anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                    text: "┘"
                    font.family: root.faceMono; font.pixelSize: 11
                    color: root.withA(root.notes.paletteAccent, 0.95)
                }
                Text {                                // final barline closes the score
                    id: ffBar
                    anchors.right: ffCorner.left; anchors.rightMargin: 4
                    anchors.verticalCenter: parent.verticalCenter
                    anchors.verticalCenterOffset: -1
                    text: "𝄂"
                    font.family: root.faceMusic; font.pixelSize: 16
                    color: root.notes.paletteAccent
                }
                Rectangle {
                    anchors.left: ffL.right; anchors.right: ffBar.left
                    anchors.leftMargin: 2; anchors.rightMargin: 6
                    anchors.verticalCenter: parent.verticalCenter
                    anchors.verticalCenterOffset: 1
                    height: 1; color: root.withA(root.notes.paletteAccent, 0.55)
                }
            }
        }
    }
}
