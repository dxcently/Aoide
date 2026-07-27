// AoideBar.qml — the dxflake-waybar homage, ENHANCED for Quickshell.
//
// Baseline identity (dxflake): a slim (~28px) full-width top strip, a dark band
// over the wallpaper, white/near-white monospace glyphs with a black text-outline
// (Text.Outline / styleColor #000000 — the legibility treatment matching the rig,
// the one literal; the glyph FILL is notes.barFg). Musical-notation workspaces,
// " / " separators, kaomoji empty-title, volume/battery/network note-glyphs.
//
//   LEFT   : 𝄞 power glyph + musical workspace cells (WorkspaceRow) + ✎ sessions.
//   CENTER : clock (click → CalendarGadget popup)  " / "  window title (kaomoji).
//   RIGHT  : / ♪~ NN% /  volume (scroll=adjust, click=mute, hover=slider)
//            • battery rests (hover=charge popout) • network glyph  (" / " seps).
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
    readonly property string sessionsPath:
        Quickshell.env("HOME") + "/Aoide/song/stage/sessions.json"
    FileView {
        id: sessionsFile
        path: root.sessionsPath
        onTextChanged: {
            try {
                var d = JSON.parse(sessionsFile.text())
                root.sessionCount = (d && d.sessions) ? d.sessions.length : 0
            } catch (e) { /* absent/garbage → hold count */ }
        }
        Component.onCompleted: sessionsFile.reload()
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

    // ══ The strip — Aero-glass (translucent barBg over compositor blur) ═════
    Rectangle {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        height: root.stripHeight
        color: root.notes.barBg
        opacity: 0.82           // glass: lets the Hyprland blur read through
    }

    // Gloss — the Win7 Aero sheen, done the CSS-trick way: a white gradient
    // laid OVER the glass, bright top half, hard stop at the midline (the
    // signature Aero "sheen line"), faint bloom at the bottom edge.
    Rectangle {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        height: root.stripHeight
        gradient: Gradient {
            GradientStop { position: 0.0;  color: Qt.rgba(1, 1, 1, 0.20) }
            GradientStop { position: 0.48; color: Qt.rgba(1, 1, 1, 0.06) }
            GradientStop { position: 0.52; color: Qt.rgba(1, 1, 1, 0.00) }
            GradientStop { position: 1.0;  color: Qt.rgba(1, 1, 1, 0.05) }
        }
    }

    // Hairline bottom edge — the bar keeps its rose glass, but the wireframe
    // hairline joins the cool field (wireCyan, base0C — falls back to accent
    // when base16 is absent).
    Rectangle {
        anchors.left: parent.left
        anchors.right: parent.right
        y: root.stripHeight - 1
        height: 1
        color: root.notes.wireCyan
        opacity: 0.5
    }

    // Pantheon depth echo: a second, dimmer wireCyan hairline 2px above the edge
    // rule. Two parallel lines read as a stacked slab (the bar's "offset
    // volume") — the bar sits on the TOP edge, so its slab projects DOWN toward
    // the screen-centre vanishing point; this is the subtle bar-scale cousin of
    // the gadgets' holoBlue depth stack.
    Rectangle {
        anchors.left: parent.left
        anchors.right: parent.right
        y: root.stripHeight - 3
        height: 1
        color: root.notes.wireCyan
        opacity: 0.2
    }

    // ── The clef power glyph — sized to sit fully inside the strip ─────────
    // A root-level sibling (NOT in the RowLayout) so its vertical centring is
    // against the strip, not a row's line box. 16px: the 𝄞 glyph paints far
    // beyond its em box (~1.8×), so this is the largest size whose full paint
    // extent still clears the 36px strip (22px clipped at the bottom).
    Text {
        id: clefText
        x: 12
        z: 2
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

    // ── Centered workspaces (khoa: "the workspaces should be in the middle")
    // — a sibling overlay at true screen-centre, the bar-scale echo of the
    // vanishing-point rule. Same overlay idiom as WorkspaceRow's activeBox.
    WorkspaceRow {
        id: centeredWorkspaces
        z: 2
        anchors.horizontalCenter: parent.horizontalCenter
        y: Math.round((root.stripHeight - height) / 2)
        notes: root.notes
    }

    RowLayout {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        height: root.stripHeight
        anchors.leftMargin: 34   // clears the clef (root sibling at x 12)
        anchors.rightMargin: 12
        spacing: 12

        // ══ LEFT: sessions + gadget tray (clef is a root sibling above) ═════
        Row {
            spacing: 12
            Layout.alignment: Qt.AlignVCenter

            // Aoide-native: live agent-sessions cell → click opens the dock.
            Text {
                anchors.verticalCenter: parent.verticalCenter
                visible: root.sessionCount > 0
                text: "✎" + root.sessionCount
                color: root.notes.paletteHot
                style: Text.Outline
                styleColor: "#000000"
                font.family: "monospace"
                font.pixelSize: 16
                font.bold: true
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

        // ══ CLOCK + TITLE — left-clustered (khoa: non-workspace elements
        // live LEFT; windows are tracked by the widgets, not the bar) ═══════
        Row {
            spacing: 6
            Layout.alignment: Qt.AlignVCenter

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

        // All slack sits between the left cluster and the status callouts.
        Item { Layout.fillWidth: true; implicitWidth: 1 }

        // ══ RIGHT: status callouts (pantheon lowercase, · separated) ═══════
        Row {
            id: rightRow
            spacing: 10
            Layout.alignment: Qt.AlignVCenter

            // Volume — "/ {icon} {pct}% /"  (muted → "/ (° × ° ) /")
            // Scroll = adjust, click = mute, hover = ASCII slider popout.
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

            // Battery — "{icon} {pct}% /"  (full → "𝆑 /"; charging → "𝄮 {pct}% /")
            // Hover = time-remaining + ASCII charge-bar popout.
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
