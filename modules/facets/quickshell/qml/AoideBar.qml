// AoideBar.qml — THE MEASURE. A clean-slate redesign in the SONG vein.
//
// Aoide is the muse of song, so the bar is one bar of music. The old
// architectural grammar (the Pantheon entablature — keystone, colonnade wings,
// architrave lintel, floating BarPane volumes) is DISCARDED wholesale. Nothing
// of that metaphor survives. In its place: a single manuscript strip on which a
// five-line staff runs the full width of the screen, and every functional cell
// is written onto that staff as notation.
//
//     ╭─ 𝄞 ── ✎ ♫▦⌁◔  hh:mm / title ─┃─ ●─●─┼─●─● ─┃─ ♫vol 𝄾bat 𝆹link ─ 𝄂 ─╮
//
//   HEAD      : 𝄞 treble clef — the key of the piece AND the powermenu key
//               (click → powermenu). It opens the staff.
//   LEFT      : ✎N agent-sessions (blocks → glitchPink pulse, a shipped tell),
//               the gadget tray (♫ now-playing · ▦ meters · ⌁ power · ◔ clock,
//               each a click-toggled BarPopout), the clock/date, the "/" and
//               the active-window title (music-kaomoji when empty).
//   CENTRE    : the workspaces, written as NOTE-HEADS on the staff line
//               (WorkspaceRow) — the melody of the measure, flanked by barlines.
//   RIGHT     : the expression marks — ♫ volume, 𝄾 battery (rests: the battery
//               empties into silence), 𝆹 network link — each hover/scroll/click
//               live, with popouts. Closed by a final barline 𝄂.
//
// Drawn structure (staff lines, barlines, playhead) carries the geometry; glyphs
// (clef, rests, note-marks) carry the ornament. It should read as sheet music.
//
// Geometry seam preserved: stripHeight stays 36 and the window's exclusiveZone
// stays 36 (shell.qml untouched), so windows never move. Everything is drawn
// INSIDE the 36px strip — no apron, no taller transparent surface (that scar,
// the wallpaper-bleed under a hairline, stays closed); the clef is sized to fit.
//
// Colours ONLY from the song (NoteState): wireCyan (base0C) rules the staff,
// clef, played note & playhead; holoBlue (base0D) the resting note-heads; violet
// (base0E) the barlines; glitchPink (base08) urgency; paletteHot the sessions
// count; barFg the text. The only sanctioned literals: the black text outline
// (#000000, legibility) and the white Aero gloss gradient (Qt.rgba(1,1,1,α)).
//
// Data sources (unchanged real Quickshell services — the plumbing survives):
//   - Hyprland   → workspaces (WorkspaceRow) + active window title.
//   - Pipewire   → default sink volume / muted.
//   - UPower     → display-device battery.
//   - Network    → /proc/net/route via FileView (files-not-processes rule).
//   - Sessions   → song/stage/sessions.json + hooks.json via FileView.

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

    // The strip is 36px; the PanelWindow reserves exactly this. Everything is
    // painted within it — the clef included — so no apron is needed and the
    // exclusiveZone stays 36 (windows do not jump).
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
        if (pct < 34) return "♩"
        if (pct < 67) return "♪"
        if (pct < 90) return "♫"
        return "♬"
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
    // Rest-notation icons by charge (𝄽 𝄾 𝄿 𝅀 𝅁 𝅂) — the battery drains toward
    // silence; full 𝆑, charging 𝄮.
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
    // spawning a process) keeps us inside the no-shell-out rule. Real iface
    // names — classified wifi vs ethernet by prefix (wl* → wifi).
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
        return "𝄽"   // a rest — the line has gone silent
    }
    function netLabel(kind) {
        if (kind === "wifi") return "wifi"
        if (kind === "eth")  return "eth"
        return "off"
    }

    // ── Window title (Hyprland active toplevel) with kaomoji empty-rewrite ──
    // A small songbook of music kaomoji combos; the empty-title rewrite rotates
    // hourly so the bar hums a different bar of the tune through the day.
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

    // Gadget tray: every non-agent gadget is its own widget spawned from a bar
    // cell. One key open at a time (click-toggled, mutually exclusive).
    property string openGadget: ""
    function toggleGadget(key) {
        openGadget = (openGadget === key) ? "" : key
    }

    // ══ MUSICAL GEOMETRY ═══════════════════════════════════════════════════
    // The staff sits at the strip's vertical midline; five lines a staffGap
    // apart. Content is written on the staff, so every cell centres on it.
    readonly property real staffMid: stripHeight / 2
    readonly property real staffGap: 3.5
    readonly property real staffSpan: staffGap * 4   // top line → bottom line
    readonly property int edgePad: 8

    // ── Inline notation vocabulary ─────────────────────────────────────────

    // A barline drawn across the staff — the violet section divider (the ┃/┼).
    component Barline: Rectangle {
        width: 1.5
        height: root.staffSpan + 4
        radius: 0.5
        color: root.notes.violet
        opacity: 0.6
        anchors.verticalCenter: parent.verticalCenter
    }

    // A tray glyph-button (♫ ▦ ⌁ ◔): active → wireCyan, else dim barFg.
    component TrayCell: Text {
        property string gkey: ""
        anchors.verticalCenter: parent.verticalCenter
        width: implicitWidth + 9
        horizontalAlignment: Text.AlignHCenter
        color: root.openGadget === gkey ? root.notes.wireCyan : root.notes.barFg
        opacity: root.openGadget === gkey ? 1.0 : 0.72
        style: Text.Outline
        styleColor: "#000000"
        font.family: "monospace"
        font.pixelSize: 16
        MouseArea {
            anchors.fill: parent
            cursorShape: Qt.PointingHandCursor
            onClicked: root.toggleGadget(parent.gkey)
        }
    }

    // ══ THE MANUSCRIPT STRIP ═══════════════════════════════════════════════
    // A single translucent page over the compositor blur — the sheet the staff
    // is printed on. Rounded ends give the ╭─ … ─╮ read of the sketch. NOT the
    // old three-volume entablature: one continuous manuscript, no floating panes.
    Rectangle {
        id: page
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        height: root.stripHeight
        radius: 8
        color: root.notes.barBg
        opacity: 0.8
    }
    // Aero gloss — the one sanctioned white gradient (bright top, sheen line).
    Rectangle {
        anchors.fill: page
        radius: 8
        gradient: Gradient {
            GradientStop { position: 0.0;  color: Qt.rgba(1, 1, 1, 0.16) }
            GradientStop { position: 0.48; color: Qt.rgba(1, 1, 1, 0.05) }
            GradientStop { position: 0.52; color: Qt.rgba(1, 1, 1, 0.00) }
            GradientStop { position: 1.0;  color: Qt.rgba(1, 1, 1, 0.04) }
        }
    }
    // The page rail — a faint wireCyan hairline framing the sheet.
    Rectangle {
        anchors.fill: page
        radius: 8
        color: "transparent"
        border.color: root.notes.wireCyan
        border.width: 1
        opacity: 0.22
    }

    // ── THE STAFF — five lines ruled the full width, at the strip midline.
    // They pass behind every cell; the note-heads (WorkspaceRow) land on them.
    Repeater {
        model: 5
        Rectangle {
            required property int index
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.leftMargin: root.edgePad + 26   // clear of the clef
            anchors.rightMargin: root.edgePad + 10
            height: 1
            y: root.staffMid + (index - 2) * root.staffGap
            color: root.notes.wireCyan
            opacity: 0.16
        }
    }

    // ══ HEAD — the treble clef, the key of the piece AND the powermenu key ══
    Text {
        id: clefText
        anchors.left: parent.left
        anchors.leftMargin: root.edgePad
        anchors.verticalCenter: parent.verticalCenter
        text: "𝄞"
        color: root.notes.wireCyan
        style: Text.Outline
        styleColor: "#000000"
        font.family: "monospace"
        font.pixelSize: 20
        font.bold: true
        MouseArea {
            anchors.fill: parent
            cursorShape: Qt.PointingHandCursor
            // Generic command through the existing bridge sender (shellbridge
            // routes it) — no new IPC machinery invented.
            onClicked: root.bridge.sendCommand({ cmd: "powermenu" })
        }
    }

    // ══ LEFT STAVE — sessions · gadget tray · clock / title ════════════════
    // Written just after the clef, reading left to right like the opening of
    // the measure. Everything centres on the staff.
    Row {
        id: leftContent
        anchors.left: clefText.right
        anchors.leftMargin: 12
        anchors.verticalCenter: parent.verticalCenter
        spacing: 9

        // Aoide-native: live agent-sessions cell → click opens the dock.
        Text {
            id: sessionsCell
            anchors.verticalCenter: parent.verticalCenter
            visible: root.sessionCount > 0
            text: "✎" + root.sessionCount
            // paletteHot at rest; when ANY session is blocked it switches to the
            // urgent role (glitchPink) and pulses — a summons from across the bar.
            color: root.anyBlocked ? root.notes.glitchPink : root.notes.paletteHot
            style: Text.Outline
            styleColor: "#000000"
            font.family: "monospace"
            font.pixelSize: 15
            font.bold: true

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

        // ── Gadget tray: ♫ now-playing · ▦ meters · ⌁ power · ◔ clock ──────
        Row {
            anchors.verticalCenter: parent.verticalCenter
            spacing: 0
            TrayCell { id: npCell;    gkey: "np";     text: "♫" }
            TrayCell { id: meterCell; gkey: "meters"; text: "▦" }
            TrayCell { id: pwrCell;   gkey: "power";  text: "⌁" }
            TrayCell { id: clkCell;   gkey: "clock";  text: "◔" }
        }

        // ── Clock + date (click → calendar popout).
        Text {
            id: clockText
            anchors.verticalCenter: parent.verticalCenter
            text: Qt.formatDateTime(root.now, "hh:mm AP  dddd MMM dd")
            color: root.calShown ? root.notes.wireCyan : root.notes.barFg
            style: Text.Outline
            styleColor: "#000000"
            font.family: "monospace"
            font.pixelSize: 14
            font.bold: true
            MouseArea {
                anchors.fill: parent
                cursorShape: Qt.PointingHandCursor
                onClicked: root.calShown = !root.calShown
            }
        }
        // The " / " separator — a slur between clock and title.
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: "/"
            color: root.notes.barFg
            style: Text.Outline
            styleColor: "#000000"
            font.family: "monospace"
            font.pixelSize: 14
            opacity: 0.75
        }
        // Active-window title (music kaomoji when empty).
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: root.winTitle()
            color: root.notes.barFg
            style: Text.Outline
            styleColor: "#000000"
            font.family: "monospace"
            font.pixelSize: 14
            font.bold: true
            elide: Text.ElideRight
        }
    }

    // ══ CENTRE — the melody: workspaces as note-heads, flanked by barlines ══
    // Held at true screen-centre (khoa's keep), so the bar's motif sits on the
    // wallpaper's axis. WorkspaceRow draws the note-heads + the playhead.
    Row {
        id: centreStave
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.verticalCenter: parent.verticalCenter
        spacing: 8

        Barline {}
        WorkspaceRow {
            id: centeredWorkspaces
            anchors.verticalCenter: parent.verticalCenter
            notes: root.notes
        }
        Barline {}
    }

    // ══ RIGHT STAVE — the expression marks, closed by a final barline ══════
    Row {
        id: rightContent
        anchors.right: parent.right
        anchors.rightMargin: root.edgePad
        anchors.verticalCenter: parent.verticalCenter
        spacing: 10

        // Volume — scroll = adjust, click = mute, hover = ASCII slider popout.
        Text {
            id: volText
            anchors.verticalCenter: parent.verticalCenter
            visible: root.volAvail
            text: root.volMuted ? "𝄽 vol" : (root.volIcon(root.volPct) + " " + root.volPct)
            color: root.volShown ? root.notes.wireCyan : root.notes.barFg
            style: Text.Outline
            styleColor: "#000000"
            font.family: "monospace"
            font.pixelSize: 14
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

        // Battery — a rest that deepens as charge drains; hover = time + bar.
        Text {
            id: battText
            anchors.verticalCenter: parent.verticalCenter
            visible: root.battAvail
            text: root.battFull
                  ? (root.battIcon() + " full")
                  : (root.battIcon() + " " + root.battPct + (root.battCharging ? "+" : ""))
            color: (root.battCrit || root.battWarn) ? root.notes.glitchPink
                                                    : root.notes.barFg
            opacity: (root.battWarn && !root.blinkOn) ? 0.3 : 1.0
            Behavior on opacity { NumberAnimation { duration: 400 } }
            style: Text.Outline
            styleColor: "#000000"
            font.family: "monospace"
            font.pixelSize: 14
            font.bold: true
            MouseArea {
                anchors.fill: parent
                hoverEnabled: true
                onEntered: root.battShown = true
                onExited: root.battShown = false
            }
        }

        // Network — a live link glows in the cool holoBlue field; a dead link
        // falls to a dim rest (𝄽) in barFg.
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: root.netGlyph(root.netKind) + " " + root.netLabel(root.netKind)
            color: root.netKind === "down" ? root.notes.barFg : root.notes.holoBlue
            opacity: root.netKind === "down" ? 0.55 : 1.0
            style: Text.Outline
            styleColor: "#000000"
            font.family: "monospace"
            font.pixelSize: 14
        }

        // The final barline — thin + thick, closing the measure (𝄂).
        Row {
            anchors.verticalCenter: parent.verticalCenter
            spacing: 2
            Rectangle {
                width: 1.5; height: root.staffSpan + 4; radius: 0.5
                color: root.notes.violet; opacity: 0.6
                anchors.verticalCenter: parent.verticalCenter
            }
            Rectangle {
                width: 3; height: root.staffSpan + 4; radius: 0.5
                color: root.notes.violet; opacity: 0.85
                anchors.verticalCenter: parent.verticalCenter
            }
        }
    }

    // ══ POPOUTS — real PopupWindows under their bar cells (BarPopout) ══════
    // Each is its own xdg_popup with GadgetFrame chrome (glass via blur_popups).
    // The cell ids above (volText/battText/clockText/np/meter/pwr/clk) anchor
    // them — every popout the entablature shipped survives, rewired unchanged.

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

    // Clock → Calendar.
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

    // ── Gadget tray popouts: each gadget is its own widget ─────────────────
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
