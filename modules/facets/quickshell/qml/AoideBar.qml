// AoideBar.qml — the dxflake-waybar homage, ENHANCED for Quickshell, then
// rebuilt in the Pantheon grammar (round 5) as an ENTABLATURE, not a slab.
//
// The old shape: one flat 36px full-width glass strip. The new shape: three
// separated floating wireframe volumes hung from the top edge — an entablature
// standing over the workspace colonnade. The KEYSTONE is the workspace pane at
// true screen-centre (the tallest block, dropping lowest, double-ruled and
// brightest) sitting exactly on the wallpaper's vanishing point. It is flanked
// by two shorter COLONNADE WINGS — the left cluster and the right callouts —
// each a bordered pane that recedes from the keystone. Every pane throws a
// holoBlue depth echo TOWARD screen-centre, its magnitude scaled to distance
// (wings lean hard in, keystone barely at all), so the bar inflects like a
// colonnade seen head-on — the same vanishing law WorkspaceRow's cells obey.
// The GAPS between panes let the wallpaper's convergence show through on
// purpose, framed by a faint full-width architrave lintel along the top edge
// and a converging tick-comb dropped into each gap. It all lives inside the
// 36px reserved band (a stepped silhouette by pane-HEIGHT, not by growing the
// window) so windows never jump and no transparent apron can bleed (round-4's
// scar). The reference: references/pantheon/67dd262c — a keystone cluster at
// top-centre, wing-panes flanking, all converging on the centre axis.
//
// Baseline identity kept (dxflake): white/near-white monospace glyphs with a
// black text-outline (Text.Outline / styleColor #000000 — the legibility
// literal; the glyph FILL is notes.barFg). Musical-notation workspaces,
// " / " separators, kaomoji empty-title, volume/battery/network note-glyphs.
//
//   LEFT WING : 𝄞 power glyph + ✎ sessions + gadget tray + clock/title.
//   KEYSTONE  : musical workspace cells (WorkspaceRow), dead screen-centre.
//   RIGHT WING: vol. · bat. · link. callouts (hover popouts, warn blink).
//
// Quickshell-native enhancements over static waybar (identity kept as baseline):
//   1. Interactivity waybar faked with tooltips → real popups: clock→calendar,
//      volume hover-slider + scroll/click, battery hover-popout.
//   2. Live animation: active-workspace box eases between cells (in WorkspaceRow),
//      urgent pulse, mpris/volume value changes are live-bound.
//   3. Aoide-native cell: live agent-sessions count (sessions.json via FileView,
//      the TerminalManagerGadget seam) → click opens the gadget dock.
//   4. Chrome: subtle Aero-glass (translucent barBg over the compositor's blur —
//      same posture as the gadget dock's frames).
//
// Data sources (real Quickshell service modules — the AoideNotifications idiom):
//   - Hyprland   → workspaces (WorkspaceRow) + active window title.
//   - Pipewire   → default sink volume / muted (degrade: hide when not ready).
//   - UPower     → display-device battery (degrade: hide on desktop / no battery).
//   - Network    → /proc/net/route via FileView (no NM service at this quickshell
//                  rev; files-not-processes → within the no-shell rule).
//   - Sessions   → song/stage/sessions.json via FileView (already-plumbed data).
// Zero hardcoded palette hex; the notation/kaomoji STRINGS are content.

import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import Quickshell.Hyprland
import Quickshell.Services.Pipewire
import Quickshell.Services.UPower

Item {
    id: root

    // ── Note + bridge dependencies (injected by shell.qml) ────────────────
    required property var notes
    required property var bridge

    // v3 geometry (khoa: "the bar is too small… don't be restricted by the
    // specifications") — a taller strip, roomier cells, larger type.
    // v5: khoa — the clef is sized to FIT the strip (the pop-out apron was
    // tried and rolled back; a taller transparent surface let the wallpaper
    // bleed through under the hairline).
    readonly property int stripHeight: 36
    implicitHeight: stripHeight

    // ── Live "now" tick for the clock (1 s) ────────────────────────────────
    property var now: new Date()
    Timer {
        interval: 1000
        repeat: true
        running: true
        onTriggered: root.now = new Date()
    }

    // ── Keep the default sink's audio subobject live ───────────────────────
    PwObjectTracker { objects: Pipewire.defaultAudioSink ? [Pipewire.defaultAudioSink] : [] }

    // ── Volume helpers (Pipewire) ──────────────────────────────────────────
    readonly property var sinkAudio: (Pipewire.ready && Pipewire.defaultAudioSink
                                       && Pipewire.defaultAudioSink.audio)
                                      ? Pipewire.defaultAudioSink.audio : null
    readonly property bool volAvail: sinkAudio !== null
    readonly property bool volMuted: sinkAudio ? sinkAudio.muted : false
    readonly property int volPct: sinkAudio ? Math.round(sinkAudio.volume * 100) : 0
    function volIcon(pct) {
        if (pct <= 0) return "𝅗𝅥"
        if (pct < 34) return "♩~"
        if (pct < 67) return "♪~"
        if (pct < 90) return "♫~"
        return "♬~"
    }
    function volAdjust(deltaPct) {
        if (!sinkAudio) return
        var v = sinkAudio.volume + deltaPct / 100
        if (v < 0) v = 0
        if (v > 1) v = 1
        sinkAudio.volume = v
    }
    function volToggleMute() {
        if (sinkAudio) sinkAudio.muted = !sinkAudio.muted
    }
    // ASCII slider for the hover popout, e.g. [▮▮▮▮▮▯▯▯▯▯] 52%
    function volSlider(pct) {
        var cells = 10
        var p = pct
        if (p < 0) p = 0
        if (p > 100) p = 100
        var filled = Math.round(p / 100 * cells)
        var s = "["
        for (var i = 0; i < cells; i++) s += (i < filled) ? "▮" : "▯"
        s += "]"
        return s
    }

    // ── Battery helpers (UPower) ───────────────────────────────────────────
    readonly property var battDev: UPower.displayDevice
    readonly property bool battAvail: battDev && battDev.isLaptopBattery && battDev.isPresent
    readonly property int battPct: battDev ? Math.round(battDev.percentage) : 0
    readonly property bool battCharging: battDev && battDev.state === UPowerDeviceState.Charging
    readonly property bool battFull: battDev && battDev.state === UPowerDeviceState.FullyCharged
    // Rest-notation icons by charge (𝄽 𝄾 𝄿 𝅀 𝅁 𝅂), full 𝆑, charging 𝄮.
    function battIcon() {
        if (battFull) return "𝆑"
        if (battCharging) return "𝄮"
        var rests = ["𝄽", "𝄾", "𝄿", "𝅀", "𝅁", "𝅂"]
        var idx = Math.floor(battPct / 100 * (rests.length - 1))
        if (idx < 0) idx = 0
        if (idx >= rests.length) idx = rests.length - 1
        return rests[idx]
    }
    function battBar(pct) {
        var cells = 10
        var p = pct
        if (p < 0) p = 0
        if (p > 100) p = 100
        var filled = Math.round(p / 100 * cells)
        var s = "["
        for (var i = 0; i < cells; i++) s += (i < filled) ? "▓" : "░"
        s += "]"
        return s
    }
    // Time-remaining string from UPower timeToEmpty / timeToFull (seconds).
    function battTime() {
        if (!battDev) return ""
        var sec = battCharging ? battDev.timeToFull : battDev.timeToEmpty
        if (!sec || sec <= 0) return ""
        var min = Math.round(sec / 60)
        var h = Math.floor(min / 60)
        var m = min % 60
        return (battCharging ? "full in " : "left ") +
               (h > 0 ? (h + "h" + (m < 10 ? "0" : "") + m) : (m + "m"))
    }
    readonly property bool battWarn: battAvail && !battCharging && battPct <= 20
    readonly property bool battCrit: battAvail && !battCharging && battPct <= 10
    // Blink toggle for warning/critical.
    property bool blinkOn: true
    Timer {
        interval: 1000
        repeat: true
        running: root.battWarn
        onTriggered: root.blinkOn = !root.blinkOn
    }

    // ── Agent-sessions count (Aoide-native — sessions.json via FileView) ────
    // Reuses the TerminalManagerGadget data seam (already-plumbed stage file).
    // Click → open the gadget dock (bridge dock verb, the AoideAgentWidgets path).
    property int sessionCount: 0
    property bool sessionsBlocked: false
    property bool hooksBlocked: false
    // A human is summoned when ANY session's merged live state is blocked —
    // either a roster state (sessions.json) OR a live hook phase (hooks.json,
    // which overrides the roster in graph.rs::merged_sessions). Either source
    // flips the ✎N cell from paletteHot to the urgent role (glitchPink) + pulse.
    readonly property bool anyBlocked: sessionsBlocked || hooksBlocked
    function anyStateBlocked(arr, key) {
        if (!arr) return false
        for (var i = 0; i < arr.length; i++) {
            var v = arr[i] ? ("" + (arr[i][key] || "")).toLowerCase() : ""
            if (v.indexOf("block") !== -1) return true
        }
        return false
    }
    readonly property string sessionsPath:
        Quickshell.env("HOME") + "/Aoide/song/stage/sessions.json"
    readonly property string hooksPath:
        Quickshell.env("HOME") + "/Aoide/song/stage/hooks.json"
    FileView {
        id: sessionsFile
        path: root.sessionsPath
        watchChanges: true
        onFileChanged: sessionsFile.reload()
        onTextChanged: {
            try {
                var d = JSON.parse(sessionsFile.text())
                root.sessionCount = (d && d.sessions) ? d.sessions.length : 0
                root.sessionsBlocked = root.anyStateBlocked(d && d.sessions, "state")
            } catch (e) { /* absent/garbage → hold count */ }
        }
        Component.onCompleted: sessionsFile.reload()
    }
    FileView {
        id: hooksFile
        path: root.hooksPath
        watchChanges: true
        onFileChanged: hooksFile.reload()
        onTextChanged: {
            try {
                var d = JSON.parse(hooksFile.text())
                root.hooksBlocked = root.anyStateBlocked(d && d.hooks, "phase")
            } catch (e) { /* absent/garbage → hold */ }
        }
        Component.onCompleted: hooksFile.reload()
    }

    // ── Network (procfs FileView — no NM service at this rev) ───────────────
    // /proc/net/route lists every iface that has a route; the default route is
    // the row whose Destination field is "00000000". Reading a kernel file (not
    // spawning a process) keeps us inside the no-shell-out rule (MeterGadget's
    // precedent). Real iface names — no hardcoded wlan0/eth0 — classified wifi vs
    // ethernet by the iface-name prefix (wl* → wifi). No essid/ipaddr from procfs
    // without shelling out → glyph only (waybar had essid only in a tooltip).
    property string netKind: "down"   // "wifi" | "eth" | "down"
    function parseRoute(text) {
        if (!text) return "down"
        var lines = ("" + text).split("\n")
        for (var i = 1; i < lines.length; i++) {
            var parts = lines[i].trim().split(/\s+/)
            if (parts.length < 2) continue
            if (parts[1] === "00000000") {
                var n = parts[0].toLowerCase()
                if (n.indexOf("wl") === 0 || n.indexOf("wlan") === 0) return "wifi"
                return "eth"
            }
        }
        return "down"
    }
    FileView {
        id: routeFile
        path: "/proc/net/route"
        onTextChanged: root.netKind = root.parseRoute(routeFile.text())
        Component.onCompleted: routeFile.reload()
    }
    Timer {
        interval: 5000
        repeat: true
        running: true
        onTriggered: routeFile.reload()
    }
    function netGlyph(kind) {
        if (kind === "wifi") return "𝆹𝅥𝅮"
        if (kind === "eth")  return "𝆺𝅥𝅯"
        return "Disconnected :c"
    }

    // ── Window title (Hyprland active toplevel) with kaomoji empty-rewrite ──
    // A small songbook of music kaomoji combos (emojicombos.com/music vocab);
    // the empty-title rewrite rotates hourly so the bar hums a different bar
    // of the tune through the day. The dxflake cat leads — it is the identity.
    readonly property var kaomojiSet: [
        "/ᐠ - ˕ -マ Ⳋ ⋆｡°✩♬ ♪",
        "♪(´▽｀) ⋆｡°✩",
        "•¨•.¸¸♪ ヾ(´〇`)ﾉ ♬",
        "(￣▽￣)/♫ •*¨*•.¸¸♪",
        "✧*。٩(ˊᗜˋ*)و ♪ ✧*。",
        "₊˚⊹ ♡ ♬ ⋆｡°✩"
    ]
    readonly property string kaomoji:
        kaomojiSet[now.getHours() % kaomojiSet.length]
    function winTitle() {
        var t = (Hyprland.activeToplevel && Hyprland.activeToplevel.title)
                ? ("" + Hyprland.activeToplevel.title) : ""
        if (t.length === 0) return kaomoji
        if (t.length > 25) return t.substring(0, 24) + "…"
        return t
    }

    // ── Popout visibility state ─────────────────────────────────────────────
    property bool calShown: false      // clock → calendar (click-toggled)
    property bool volShown: false      // volume hover slider
    property bool battShown: false     // battery hover popout

    // Gadget tray (v2 restructure): every non-agent gadget is its own widget
    // spawned from a bar cell. One key open at a time (click-toggled,
    // mutually exclusive); "" = all closed.
    property string openGadget: ""
    function toggleGadget(key) {
        openGadget = (openGadget === key) ? "" : key
    }

    // ══ THE ENTABLATURE ════════════════════════════════════════════════════
    // Three floating volumes, no continuous slab. Geometry lives inside the
    // 36px reserved band; the stepped silhouette is by pane HEIGHT, not window
    // height — so exclusiveZone stays 36 and windows never jump. Screen-centre
    // is root.width/2 (the bar spans the screen); the keystone sits on it, the
    // wings recede from it, every echo leans toward it.
    readonly property real screenCenter: width / 2
    readonly property int keystoneH: 34   // the emphasised central block (deepest)
    readonly property int wingH: 28       // the flanking wings (raised, shorter)

    // ── BarPane: one floating wireframe volume ─────────────────────────────
    // Aero-glass body + a Win7 sheen, wrapped in a wireCyan wireframe border
    // (the pantheon "hollow volume" read), with a holoBlue depth back-copy
    // thrown TOWARD screen-centre — the offset volume of the depth recipe, at
    // bar scale. `keystone: true` double-rules the border and brightens it, so
    // the centre block reads as the emphasised wedge. `echoDx` is the lean
    // toward centre (sign + magnitude computed by each instance from its own
    // distance off the axis); the body Item is the default slot.
    component BarPane: Item {
        id: pane
        property real echoDx: 0
        property real echoDy: 2
        property bool keystone: false
        default property alias body: bodySlot.data

        // Depth echo — the holoBlue back-copy, offset toward the vanishing
        // point. Declared FIRST so it renders behind the glass; only the sliver
        // past the pane's edge shows as a clean outline (the depth recipe).
        Rectangle {
            x: pane.echoDx
            y: pane.echoDy
            width: pane.width
            height: pane.height
            radius: 3
            color: "transparent"
            border.color: root.notes.holoBlue
            border.width: 1
            opacity: pane.keystone ? 0.30 : 0.24
        }

        // Aero-glass body (translucent barBg over the compositor's blur).
        Rectangle {
            anchors.fill: parent
            radius: 3
            color: root.notes.barBg
            opacity: 0.82
        }

        // Gloss — the Win7 sheen, the CSS-trick way: bright top half, a hard
        // stop at the midline (the signature Aero "sheen line"), a faint bloom
        // at the foot. The whites are the one literal, matching the rig.
        Rectangle {
            anchors.fill: parent
            radius: 3
            gradient: Gradient {
                GradientStop { position: 0.0;  color: Qt.rgba(1, 1, 1, 0.20) }
                GradientStop { position: 0.48; color: Qt.rgba(1, 1, 1, 0.06) }
                GradientStop { position: 0.52; color: Qt.rgba(1, 1, 1, 0.00) }
                GradientStop { position: 1.0;  color: Qt.rgba(1, 1, 1, 0.05) }
            }
        }

        // Wireframe front border — the hollow-volume outline (wireCyan, base0C).
        // The keystone blazes brighter than the wings (neon dominance: one
        // block leads the field).
        Rectangle {
            anchors.fill: parent
            radius: 3
            color: "transparent"
            border.color: root.notes.wireCyan
            border.width: 1
            opacity: pane.keystone ? 0.7 : 0.5
        }
        // Keystone double-rule — a second inset border, the "emphasised wedge"
        // tell borrowed from the DAG's project volumes.
        Rectangle {
            visible: pane.keystone
            anchors.fill: parent
            anchors.margins: 3
            radius: 2
            color: "transparent"
            border.color: root.notes.wireCyan
            border.width: 1
            opacity: 0.3
        }

        Item { id: bodySlot; anchors.fill: parent }
    }

    // ── The architrave lintel — a faint full-width rail along the top edge.
    // It is the entablature's top member: it ties the three blocks into one
    // structure WITHOUT re-becoming a slab, and it turns the between-pane gaps
    // into framed openings rather than a torn strip (round-4's lesson: exposed
    // wallpaper must read as intended). wireCyan, low opacity.
    Rectangle {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        height: 1
        color: root.notes.wireCyan
        opacity: 0.22
        z: 3
    }

    // ── GapMark: a tiny converging tick-comb dropped from the lintel into a
    // gap — the reference's leader-tick vocabulary, framing the wallpaper
    // reveal as a grille. Centre tick tallest; the pair lean toward it.
    component GapMark: Row {
        spacing: 3
        z: 3
        Repeater {
            model: 3
            Rectangle {
                required property int index
                width: 1
                height: index === 1 ? 9 : 5
                color: root.notes.wireCyan
                opacity: index === 1 ? 0.4 : 0.22
            }
        }
    }

    // ══ LEFT WING — the port-side colonnade block ══════════════════════════
    // 𝄞 power · ✎N sessions · gadget tray · clock/title. Hugs its content and
    // hangs from the top-left; its echo leans RIGHT toward the keystone.
    BarPane {
        id: leftWing
        anchors.left: parent.left
        anchors.leftMargin: 6
        anchors.top: parent.top
        height: root.wingH
        width: leftContent.implicitWidth + 20
        // Lean toward screen-centre, magnitude by distance (clamped like the
        // colonnade cells). Left of centre → positive (rightward) lean.
        echoDx: Math.max(0, Math.min(4, (root.screenCenter - (x + width / 2)) * 0.02))
        echoDy: 3
        z: 2

        Row {
            id: leftContent
            anchors.verticalCenter: parent.verticalCenter
            anchors.left: parent.left
            anchors.leftMargin: 10
            spacing: 10

            // The clef power glyph — 𝄞 IS the power-menu button (it has a job).
            // 16px: the glyph paints ~1.8× its em box, so this is the largest
            // size that clears the wing's height.
            Text {
                id: clefText
                anchors.verticalCenter: parent.verticalCenter
                text: "𝄞"
                color: root.notes.barFg
                style: Text.Outline
                styleColor: "#000000"
                font.family: "monospace"
                font.pixelSize: 16
                font.bold: true
                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    // Generic command object through the existing bridge sender
                    // (no new QML IPC machinery invented — shellbridge routes it).
                    onClicked: root.bridge.sendCommand({ cmd: "powermenu" })
                }
            }

            // Aoide-native: live agent-sessions cell → click opens the dock.
            Text {
                id: sessionsCell
                anchors.verticalCenter: parent.verticalCenter
                visible: root.sessionCount > 0
                text: "✎" + root.sessionCount
                // Blazes paletteHot at rest; when ANY session is blocked the cell
                // switches to the urgent role (glitchPink) and pulses — a human
                // summons the eye can't miss from across the bar.
                color: root.anyBlocked ? root.notes.glitchPink : root.notes.paletteHot
                style: Text.Outline
                styleColor: "#000000"
                font.family: "monospace"
                font.pixelSize: 16
                font.bold: true

                // Urgent pulse (blink_red homage from WorkspaceRow): 600ms
                // InOutQuad breath to 0.35 and back, forever, only while blocked.
                SequentialAnimation on opacity {
                    running: root.anyBlocked
                    loops: Animation.Infinite
                    NumberAnimation { to: 0.35; duration: 600; easing.type: Easing.InOutQuad }
                    NumberAnimation { to: 1.0;  duration: 600; easing.type: Easing.InOutQuad }
                }

                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: root.bridge.sendCommand({ cmd: "dock", action: "toggle" })
                }
            }

            // ── Gadget tray: one cell per bar-spawned widget ────────────────
            // ♫ now-playing · ▦ meters · ⌁ power · ◔ clock. Click toggles the
            // cell's BarPopout (mutually exclusive via openGadget). Accent
            // color while open — same active treatment as the clock cell.
            Row {
                anchors.verticalCenter: parent.verticalCenter
                spacing: 0
                Text {
                    id: npCell
                    anchors.verticalCenter: parent.verticalCenter
                    text: "♫"
                    width: implicitWidth + 10
                    horizontalAlignment: Text.AlignHCenter
                    color: root.openGadget === "np" ? root.notes.wireCyan : root.notes.barFg
                    opacity: root.openGadget === "np" ? 1.0 : 0.75
                    style: Text.Outline
                    styleColor: "#000000"
                    font.family: "monospace"
                    font.pixelSize: 17
                    MouseArea {
                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        onClicked: root.toggleGadget("np")
                    }
                }
                Text {
                    id: meterCell
                    anchors.verticalCenter: parent.verticalCenter
                    text: "▦"
                    width: implicitWidth + 10
                    horizontalAlignment: Text.AlignHCenter
                    color: root.openGadget === "meters" ? root.notes.wireCyan : root.notes.barFg
                    opacity: root.openGadget === "meters" ? 1.0 : 0.75
                    style: Text.Outline
                    styleColor: "#000000"
                    font.family: "monospace"
                    font.pixelSize: 17
                    MouseArea {
                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        onClicked: root.toggleGadget("meters")
                    }
                }
                Text {
                    id: pwrCell
                    anchors.verticalCenter: parent.verticalCenter
                    text: "⌁"
                    width: implicitWidth + 10
                    horizontalAlignment: Text.AlignHCenter
                    color: root.openGadget === "power" ? root.notes.wireCyan : root.notes.barFg
                    opacity: root.openGadget === "power" ? 1.0 : 0.75
                    style: Text.Outline
                    styleColor: "#000000"
                    font.family: "monospace"
                    font.pixelSize: 17
                    MouseArea {
                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        onClicked: root.toggleGadget("power")
                    }
                }
                Text {
                    id: clkCell
                    anchors.verticalCenter: parent.verticalCenter
                    text: "◔"
                    width: implicitWidth + 10
                    horizontalAlignment: Text.AlignHCenter
                    color: root.openGadget === "clock" ? root.notes.wireCyan : root.notes.barFg
                    opacity: root.openGadget === "clock" ? 1.0 : 0.75
                    style: Text.Outline
                    styleColor: "#000000"
                    font.family: "monospace"
                    font.pixelSize: 17
                    MouseArea {
                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        onClicked: root.toggleGadget("clock")
                    }
                }
            }

            // ── Clock + title (khoa: non-workspace elements live LEFT;
            // windows are tracked by the widgets, not the bar).
            Text {
                id: clockText
                anchors.verticalCenter: parent.verticalCenter
                text: Qt.formatDateTime(root.now, "hh:mm AP  dddd MMM dd")
                color: root.calShown ? root.notes.wireCyan : root.notes.barFg
                style: Text.Outline
                styleColor: "#000000"
                font.family: "monospace"
                font.pixelSize: 15
                font.bold: true
                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: root.calShown = !root.calShown
                }
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: "/"
                color: root.notes.barFg
                style: Text.Outline
                styleColor: "#000000"
                font.family: "monospace"
                font.pixelSize: 15
                opacity: 0.8
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: root.winTitle()
                color: root.notes.barFg
                style: Text.Outline
                styleColor: "#000000"
                font.family: "monospace"
                font.pixelSize: 15
                font.bold: true
                elide: Text.ElideRight
            }
        }
    }

    // ══ KEYSTONE — the workspace pane at true screen-centre ════════════════
    // The tallest block, dropping lowest, double-ruled and brightest — the
    // wedge on the vanishing point. Its own echo leans barely at all (it IS the
    // centre), so the row of echoes converges here. WorkspaceRow's cells throw
    // their own holoBlue echoes onto this same axis (the colonnade beneath the
    // entablature) — the languages meet.
    BarPane {
        id: keystone
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.top: parent.top
        height: root.keystoneH
        width: centeredWorkspaces.implicitWidth + 18
        keystone: true
        echoDx: 0
        echoDy: 2
        z: 2

        WorkspaceRow {
            id: centeredWorkspaces
            anchors.centerIn: parent
            notes: root.notes
        }
    }

    // ══ RIGHT WING — the starboard callout block ═══════════════════════════
    // vol. · bat. · link. Hugs content, hangs from the top-right; its echo
    // leans LEFT toward the keystone (mirror of the left wing).
    BarPane {
        id: rightWing
        anchors.right: parent.right
        anchors.rightMargin: 6
        anchors.top: parent.top
        height: root.wingH
        width: rightContent.implicitWidth + 20
        echoDx: Math.max(-4, Math.min(0, (root.screenCenter - (x + width / 2)) * 0.02))
        echoDy: 3
        z: 2

        Row {
            id: rightContent
            anchors.verticalCenter: parent.verticalCenter
            anchors.right: parent.right
            anchors.rightMargin: 10
            spacing: 10

            // Volume — scroll = adjust, click = mute, hover = ASCII slider popout.
            Text {
                id: volText
                anchors.verticalCenter: parent.verticalCenter
                visible: root.volAvail
                text: root.volMuted ? "vol.muted" : ("vol." + root.volPct)
                color: root.notes.barFg
                style: Text.Outline
                styleColor: "#000000"
                font.family: "monospace"
                font.pixelSize: 15
                font.bold: true
                MouseArea {
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onEntered: root.volShown = true
                    onExited: root.volShown = false
                    onClicked: root.volToggleMute()
                    onWheel: {
                        root.volAdjust(wheel.angleDelta.y > 0 ? 2 : -2)
                        wheel.accepted = true
                    }
                }
            }

            // Battery — hover = time-remaining + ASCII charge-bar popout.
            Text {
                id: battText
                anchors.verticalCenter: parent.verticalCenter
                visible: root.battAvail
                text: root.battFull
                      ? "bat.full"
                      : ("bat." + root.battPct + (root.battCharging ? "+" : ""))
                color: (root.battCrit || root.battWarn) ? root.notes.glitchPink
                                                        : root.notes.barFg
                opacity: (root.battWarn && !root.blinkOn) ? 0.3 : 1.0
                Behavior on opacity { NumberAnimation { duration: 400 } }
                style: Text.Outline
                styleColor: "#000000"
                font.family: "monospace"
                font.pixelSize: 15
                font.bold: true
                MouseArea {
                    anchors.fill: parent
                    hoverEnabled: true
                    onEntered: root.battShown = true
                    onExited: root.battShown = false
                }
            }

            // Network callout — a live link reads in the cool wireframe field
            // (holoBlue, base0D); a dead link recedes to dim barFg. vol./bat.
            // stay barFg so their changing digits keep maximum legibility.
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: "link." + root.netKind
                color: root.netKind === "down" ? root.notes.barFg : root.notes.holoBlue
                opacity: root.netKind === "down" ? 0.55 : 1.0
                style: Text.Outline
                styleColor: "#000000"
                font.family: "monospace"
                font.pixelSize: 15
            }
        }
    }

    // ── The framed gaps — a tick-comb hung from the lintel at each opening
    // between the wings and the keystone. Positioned at the gap midpoints so
    // the wallpaper's convergence reads through a deliberate grille, not a tear.
    GapMark {
        y: 1
        x: Math.round(((leftWing.x + leftWing.width) + keystone.x) / 2) - 4
    }
    GapMark {
        y: 1
        x: Math.round(((keystone.x + keystone.width) + rightWing.x) / 2) - 4
    }

    // ══ POPOUTS — real PopupWindows under their bar cells (BarPopout) ══════
    // The v1 in-Item Rectangles drew at y > 28 inside the 28px bar surface —
    // clipped by the layer surface, never rendered. Each popout is now its
    // own xdg_popup with GadgetFrame chrome (glass via blur_popups rule).

    // Volume hover slider.
    BarPopout {
        notes: root.notes
        cell: volText
        title: "volume.level"
        popoutWidth: 180
        shown: root.volShown && root.volAvail
        Text {
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            text: root.volMuted ? "muted" : (root.volSlider(root.volPct) + " " + root.volPct + "%")
            color: root.notes.barFg
            font.family: "monospace"
            font.pixelSize: 12
        }
    }

    // Battery hover popout — time-remaining + charge bar.
    BarPopout {
        notes: root.notes
        cell: battText
        title: "battery.gauge"
        popoutWidth: 180
        shown: root.battShown && root.battAvail
        Column {
            width: parent.width
            spacing: 2
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                text: root.battBar(root.battPct) + " " + root.battPct + "%"
                color: root.battWarn ? root.notes.paletteUrgent : root.notes.barFg
                font.family: "monospace"
                font.pixelSize: 12
            }
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                visible: root.battTime().length > 0
                text: root.battTime()
                color: root.notes.barFg
                opacity: 0.75
                font.family: "monospace"
                font.pixelSize: 11
            }
        }
    }

    // Clock → Calendar (the Quickshell-native upgrade of waybar's tooltip).
    BarPopout {
        notes: root.notes
        cell: clockText
        title: "calendar.sheet"
        popoutWidth: 220
        shown: root.calShown
        CalendarGadget {
            width: parent.width
            notes: root.notes
        }
    }

    // ── Gadget tray popouts (v2): each gadget is its own widget ────────────
    BarPopout {
        notes: root.notes
        cell: npCell
        title: "nowplaying.score"
        shown: root.openGadget === "np"
        NowPlayingGadget {
            width: parent.width
            notes: root.notes
        }
    }
    BarPopout {
        notes: root.notes
        cell: meterCell
        title: "meters.pulse"
        shown: root.openGadget === "meters"
        MeterGadget {
            width: parent.width
            notes: root.notes
        }
    }
    BarPopout {
        notes: root.notes
        cell: pwrCell
        title: "power.reserve"
        shown: root.openGadget === "power"
        PowerGadget {
            width: parent.width
            notes: root.notes
        }
    }
    BarPopout {
        notes: root.notes
        cell: clkCell
        title: "clock.face"
        shown: root.openGadget === "clock"
        ClockGadget {
            width: parent.width
            notes: root.notes
        }
    }
}
