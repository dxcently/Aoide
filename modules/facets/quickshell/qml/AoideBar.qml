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
import Quickshell.Io
import Quickshell.Hyprland
import Quickshell.Services.Pipewire
import Quickshell.Services.UPower

Item {
    id: root

    // ── Note + bridge dependencies (injected by shell.qml) ────────────────
    required property var notes
    required property var bridge

    implicitHeight: 28

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
    readonly property string sessionsPath: Qt.resolvedUrl(
        (StandardPaths.writableLocation(StandardPaths.HomeLocation)) +
        "/Aoide/song/stage/sessions.json")
    FileView {
        id: sessionsFile
        path: root.sessionsPath
        onTextChanged: {
            try {
                var d = JSON.parse(sessionsFile.text)
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
        onTextChanged: root.netKind = root.parseRoute(routeFile.text)
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
    readonly property string kaomoji: "/ᐠ - ˕ -マ Ⳋ ⋆｡°✩♬ ♪"
    function winTitle() {
        var t = (Hyprland.activeToplevel && Hyprland.activeToplevel.title)
                ? ("" + Hyprland.activeToplevel.title) : ""
        if (t.length === 0) return kaomoji
        if (t.length > 25) return t.substring(0, 24) + "…"
        return t
    }

    // ── Popout visibility state (mutually exclusive-ish; each hover-gated) ──
    property bool calShown: false      // clock → calendar (click-toggled)
    property bool volShown: false      // volume hover slider
    property bool battShown: false     // battery hover popout

    // ══ The strip — subtle Aero-glass (translucent barBg over compositor blur) ══
    Rectangle {
        anchors.fill: parent
        color: root.notes.barBg
        opacity: 0.82           // glass: lets the Hyprland blur read through

        // Hairline bottom edge.
        Rectangle {
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            height: 1
            color: root.notes.barAccent
            opacity: 0.5
        }
    }

    RowLayout {
        anchors.fill: parent
        anchors.leftMargin: 8
        anchors.rightMargin: 8
        spacing: 8

        // ══ LEFT: power glyph + workspaces + sessions ══════════════════════
        Row {
            spacing: 8
            Layout.alignment: Qt.AlignVCenter

            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: "𝄞"
                color: root.notes.barFg
                style: Text.Outline
                styleColor: "#000000"
                font.family: "monospace"
                font.pixelSize: 18
                font.bold: true
                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    // Generic command object through the existing bridge sender
                    // (no new QML IPC machinery invented — shellbridge routes it).
                    onClicked: root.bridge.sendCommand({ cmd: "powermenu" })
                }
            }

            WorkspaceRow {
                anchors.verticalCenter: parent.verticalCenter
                notes: root.notes
            }

            // Aoide-native: live agent-sessions cell → click opens the dock.
            Text {
                anchors.verticalCenter: parent.verticalCenter
                visible: root.sessionCount > 0
                text: "✎" + root.sessionCount
                color: root.notes.barAccent
                style: Text.Outline
                styleColor: "#000000"
                font.family: "monospace"
                font.pixelSize: 14
                font.bold: true
                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: root.bridge.sendCommand({ cmd: "dock", action: "toggle" })
                }
            }
        }

        // ══ CENTER: clock (→ calendar popup)  /  window title ══════════════
        Item { Layout.fillWidth: true; implicitWidth: 1 }

        Row {
            spacing: 6
            Layout.alignment: Qt.AlignVCenter

            Text {
                id: clockText
                anchors.verticalCenter: parent.verticalCenter
                text: Qt.formatDateTime(root.now, "hh:mm AP  dddd MMM dd")
                color: root.calShown ? root.notes.barAccent : root.notes.barFg
                style: Text.Outline
                styleColor: "#000000"
                font.family: "monospace"
                font.pixelSize: 13
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
                font.pixelSize: 13
                opacity: 0.8
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: root.winTitle()
                color: root.notes.barFg
                style: Text.Outline
                styleColor: "#000000"
                font.family: "monospace"
                font.pixelSize: 13
                font.bold: true
                elide: Text.ElideRight
            }
        }

        Item { Layout.fillWidth: true; implicitWidth: 1 }

        // ══ RIGHT: volume  battery  network (" / " seps) ═══════════════════
        Row {
            id: rightRow
            spacing: 6
            Layout.alignment: Qt.AlignVCenter

            // Volume — "/ {icon} {pct}% /"  (muted → "/ (° × ° ) /")
            // Scroll = adjust, click = mute, hover = ASCII slider popout.
            Text {
                id: volText
                anchors.verticalCenter: parent.verticalCenter
                visible: root.volAvail
                text: root.volMuted
                      ? "/ (° × ° ) /"
                      : ("/ " + root.volIcon(root.volPct) + " " + root.volPct + "% /")
                color: root.notes.barFg
                style: Text.Outline
                styleColor: "#000000"
                font.family: "monospace"
                font.pixelSize: 13
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
                      ? "𝆑 /"
                      : (root.battIcon() + " " + root.battPct + "% /")
                color: (root.battCrit || root.battWarn) ? root.notes.paletteUrgent
                                                        : root.notes.barFg
                opacity: (root.battWarn && !root.blinkOn) ? 0.3 : 1.0
                Behavior on opacity { NumberAnimation { duration: 400 } }
                style: Text.Outline
                styleColor: "#000000"
                font.family: "monospace"
                font.pixelSize: 13
                font.bold: true
                MouseArea {
                    anchors.fill: parent
                    hoverEnabled: true
                    onEntered: root.battShown = true
                    onExited: root.battShown = false
                }
            }

            // Network glyph — "{glyph} /"
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: root.netGlyph(root.netKind) + " /"
                color: root.notes.barFg
                opacity: root.netKind === "down" ? 0.55 : 1.0
                style: Text.Outline
                styleColor: "#000000"
                font.family: "monospace"
                font.pixelSize: 13
            }
        }
    }

    // ══ POPOUTS (anchored under the bar; glass panels, notes-only) ═════════

    // Volume hover slider — anchored under the volume cell.
    Rectangle {
        visible: root.volShown && root.volAvail
        x: Math.min(root.width - width - 8,
                    Math.max(8, volText.mapToItem(root, 0, 0).x - 8))
        y: root.height + 2
        width: volSliderText.implicitWidth + 16
        height: volSliderText.implicitHeight + 12
        radius: 4
        color: root.notes.barBg
        opacity: 0.92
        border.color: root.notes.barAccent
        border.width: 1
        Text {
            id: volSliderText
            anchors.centerIn: parent
            text: root.volMuted ? "muted" : (root.volSlider(root.volPct) + " " + root.volPct + "%")
            color: root.notes.barFg
            font.family: "monospace"
            font.pixelSize: 12
        }
    }

    // Battery hover popout — time-remaining + charge bar.
    Rectangle {
        visible: root.battShown && root.battAvail
        x: Math.min(root.width - width - 8,
                    Math.max(8, battText.mapToItem(root, 0, 0).x - 8))
        y: root.height + 2
        width: battCol.implicitWidth + 16
        height: battCol.implicitHeight + 12
        radius: 4
        color: root.notes.barBg
        opacity: 0.92
        border.color: root.notes.barAccent
        border.width: 1
        Column {
            id: battCol
            anchors.centerIn: parent
            spacing: 2
            Text {
                text: root.battBar(root.battPct) + " " + root.battPct + "%"
                color: root.battWarn ? root.notes.paletteUrgent : root.notes.barFg
                font.family: "monospace"
                font.pixelSize: 12
            }
            Text {
                visible: root.battTime().length > 0
                text: root.battTime()
                color: root.notes.barFg
                opacity: 0.75
                font.family: "monospace"
                font.pixelSize: 11
            }
        }
    }

    // Clock → Calendar popup (the ASCII month grid dropped under the bar —
    // the Quickshell-native upgrade of waybar's calendar tooltip).
    Rectangle {
        visible: root.calShown
        x: Math.min(root.width - width - 8,
                    Math.max(8, clockText.mapToItem(root, 0, 0).x - 8))
        y: root.height + 2
        width: calGadget.implicitWidth + 16
        height: calGadget.implicitHeight + 12
        radius: 4
        color: root.notes.barBg
        opacity: 0.92
        border.color: root.notes.barAccent
        border.width: 1
        CalendarGadget {
            id: calGadget
            anchors.centerIn: parent
            notes: root.notes
        }
    }
}
