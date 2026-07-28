// AoideBar.qml — THE MEASURE. A clean-slate redesign in the SONG vein.
//
// Aoide is the muse of song, so the bar is one bar of music. The old
// architectural grammar (the Pantheon entablature — keystone, colonnade wings,
// architrave lintel, floating BarPane volumes) is DISCARDED wholesale. Nothing
// of that metaphor survives. In its place: a single manuscript strip on which a
// five-line staff runs the full width of the screen, and every functional cell
// is written onto that staff as notation.
//
//     ╭─ 𝄞 ── ✎ ♫  title ─┃─ ●─●─┼─●─● ─┃─ ♫vol 𝄾bat 𝆹link ─ 𝄂 ─╮
//
//   HEAD      : 𝄞 treble clef — the key of the piece AND the powermenu key
//               (click → powermenu). It opens the staff.
//   LEFT      : ✎N agent-sessions (blocks → glitchPink pulse, a shipped tell),
//               the gadget tray (♫ now-playing, a click-toggled BarPopout —
//               meters/power/clock moved to the AoideAgentWidgets dock, seated
//               at its bottom below the agent pair), and the active-window
//               title (music-kaomoji when empty).
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
// Colour: the WHITE MUSIC SHEET — a Win7 Aero taskbar rendered as manuscript
// paper. The page is an OPAQUE white sheet (Qt.rgba(1,1,1,0.92)) with the white
// gloss gradient riding on top (the Aero highlight); the hyprglass blur behind
// it gives faint depth but the strip reads solid. The structural ink stays BLACK
// (#000000 staff lines, barlines, playhead), but the TEXT ink is now the song's
// umber (notes.paletteFg #423420) drawn CLEAN — the old white legibility outline
// is dropped, since dark text on the cream sheet needs no halo (that outline was
// a relic of the old dark bar and only muddied the type on cream).
// One restrained STATE accent survives from the song (NoteState): the ACTIVE
// workspace note-head fills with paletteAccent, the BLOCKED ✎ pulse + low battery
// go glitchPink, and open/hover toggles (clock→calendar, volume, tray) flash
// paletteAccent. Everything at rest is black.
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

    // ── Live "now" tick (1 s) — drives the hourly kaomoji rotation below ────
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
    property bool volShown: false      // volume hover slider
    property bool battShown: false     // battery hover popout

    // Gadget tray: now-playing is the sole bar-spawned gadget (meters/power
    // live in the AoideAgentWidgets dock instead; the clock/date stay on the
    // bar as a plain readout). Click-toggled.
    property string openGadget: ""
    function toggleGadget(key) {
        openGadget = (openGadget === key) ? "" : key
    }
    // Clock → calendar sheet (click-toggled, its own popout below).
    property bool calShown: false

    // ══ MUSICAL GEOMETRY ═══════════════════════════════════════════════════
    // The staff sits at the strip's vertical midline; five lines a staffGap
    // apart. Content is written on the staff, so every cell centres on it.
    readonly property real staffMid: stripHeight / 2
    readonly property real staffGap: 3.5
    readonly property real staffSpan: staffGap * 4   // top line → bottom line
    readonly property int edgePad: 8

    // ── Inline notation vocabulary ─────────────────────────────────────────

    // A barline drawn across the staff — a black engraved section divider (┃/┼).
    component Barline: Rectangle {
        width: 1.5
        height: root.staffSpan + 4
        radius: 0.5
        color: "#000000"
        opacity: 0.7
        anchors.verticalCenter: parent.verticalCenter
    }

    // A tray glyph-button (♫ ▦ ⌁ ◔): active → accent, else resting black ink.
    component TrayCell: Text {
        property string gkey: ""
        anchors.verticalCenter: parent.verticalCenter
        width: implicitWidth + 9
        horizontalAlignment: Text.AlignHCenter
        color: root.openGadget === gkey ? root.notes.paletteAccent : root.notes.paletteFg
        opacity: root.openGadget === gkey ? 1.0 : 0.92
        font.family: "monospace"
        font.pixelSize: 14
        MouseArea {
            anchors.fill: parent
            cursorShape: Qt.PointingHandCursor
            onClicked: root.toggleGadget(parent.gkey)
        }
    }

    // ══ THE MANUSCRIPT STRIP ═══════════════════════════════════════════════
    // A single OPAQUE WHITE Aero-glass page — the Win7 taskbar as a sheet of
    // manuscript paper. High-opacity white fill (the hyprglass blur still sits
    // behind it for faint Aero depth, but the sheet reads as a solid glossy
    // strip, not see-through). Rounded ends give the ╭─ … ─╮ read of the sketch.
    Rectangle {
        id: page
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        height: root.stripHeight
        radius: 0                        // EDGED — hard square corners, no round
        // Cream frosted glass at 0.72 — MATCHES the terminal's paletteBg tint
        // and opacity (kitty 0.72), so bar and terminals read as one glass.
        color: Qt.rgba(Qt.color(root.notes.paletteBg).r,
                       Qt.color(root.notes.paletteBg).g,
                       Qt.color(root.notes.paletteBg).b, 0.72)
        opacity: 1.0
    }
    // Aero gloss — the sanctioned white sheen (bright top, hard midline stop),
    // the glossy Win7 highlight riding on top of the white sheet.
    Rectangle {
        anchors.fill: page
        radius: 0
        gradient: Gradient {
            GradientStop { position: 0.0;  color: Qt.rgba(1, 1, 1, 0.20) }
            GradientStop { position: 0.48; color: Qt.rgba(1, 1, 1, 0.06) }
            GradientStop { position: 0.52; color: Qt.rgba(1, 1, 1, 0.00) }
            GradientStop { position: 1.0;  color: Qt.rgba(1, 1, 1, 0.05) }
        }
    }
    // The page rail — a DEFINED black edge framing the transparent strip.
    Rectangle {
        anchors.fill: page
        radius: 0
        color: "transparent"
        border.color: "#000000"
        border.width: 1
        opacity: 0.5
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
            color: "#000000"
            opacity: 0.55
        }
    }

    // ══ HEAD — the treble clef, the key of the piece AND the powermenu key ══
    Text {
        id: clefText
        anchors.left: parent.left
        anchors.leftMargin: root.edgePad
        anchors.verticalCenter: parent.verticalCenter
        text: "𝄞"
        color: root.notes.paletteFg
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
            // Black ink at rest; when ANY session is blocked it switches to the
            // urgent role (glitchPink) and pulses — a summons from across the bar.
            color: root.anyBlocked ? root.notes.glitchPink : root.notes.paletteFg
            font.family: "monospace"
            font.pixelSize: 13
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

        // ── Gadget tray: ♫ now-playing (sole bar-spawned gadget) ───────────
        Row {
            anchors.verticalCenter: parent.verticalCenter
            spacing: 0
            TrayCell { id: npCell; gkey: "np"; text: "♫" }
        }

        // ── Clock + date (click → calendar popout). Lives on the bar, not the
        // dock — the ambient face belongs beside the measure, not in the case.
        Text {
            id: clockText
            anchors.verticalCenter: parent.verticalCenter
            text: Qt.formatDateTime(root.now, "hh:mm AP  dddd MMM dd")
            color: root.calShown ? root.notes.paletteAccent : root.notes.paletteFg
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
            color: root.notes.paletteFg
            font.family: "monospace"
            font.pixelSize: 14
            opacity: 0.55
        }
        // Active-window title (music kaomoji when empty).
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: root.winTitle()
            color: root.notes.paletteFg
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
            color: root.volShown ? root.notes.paletteAccent : root.notes.paletteFg
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
                                                    : root.notes.paletteFg
            opacity: (root.battWarn && !root.blinkOn) ? 0.3 : 1.0
            Behavior on opacity { NumberAnimation { duration: 400 } }
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

        // Network — black ink; a live link is full-strength, a dead link falls
        // to a dim rest (𝄽) at reduced opacity.
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: root.netGlyph(root.netKind) + " " + root.netLabel(root.netKind)
            color: root.notes.paletteFg
            opacity: root.netKind === "down" ? 0.5 : 1.0
            font.family: "monospace"
            font.pixelSize: 14
        }

        // The final barline — thin + thick black rules, closing the measure (𝄂).
        Row {
            anchors.verticalCenter: parent.verticalCenter
            spacing: 2
            Rectangle {
                width: 1.5; height: root.staffSpan + 4; radius: 0.5
                color: "#000000"; opacity: 0.7
                anchors.verticalCenter: parent.verticalCenter
            }
            Rectangle {
                width: 3; height: root.staffSpan + 4; radius: 0.5
                color: "#000000"; opacity: 0.9
                anchors.verticalCenter: parent.verticalCenter
            }
        }
    }

    // ══ POPOUTS — real PopupWindows under their bar cells (BarPopout) ══════
    // Each is its own xdg_popup with GadgetFrame chrome (glass via blur_popups).
    // The cell ids above (volText/battText/npCell) anchor them. Meters/power/
    // clock popouts moved to the AoideAgentWidgets dock (bottom-seated frames).

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
            color: "#14141a"
            font.family: "monospace"
            font.pixelSize: 14
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
                color: root.battWarn ? root.notes.paletteUrgent : "#14141a"
                font.family: "monospace"
                font.pixelSize: 14
            }
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                visible: root.battTime().length > 0
                text: root.battTime()
                color: "#14141a"
                opacity: 0.75
                font.family: "monospace"
                font.pixelSize: 11
            }
        }
    }

    // ── Gadget tray popout: now-playing is the sole bar-spawned gadget ──────
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

    // ── Clock → Calendar sheet (bound to the bar's clock/date readout) ──────
    BarPopout {
        notes: root.notes
        cell: clockText
        title: "calendar.sheet"
        shown: root.calShown
        CalendarGadget {
            width: parent.width
            notes: root.notes
        }
    }
}
