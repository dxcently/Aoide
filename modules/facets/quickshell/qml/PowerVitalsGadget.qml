import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Services.UPower

// ── THE POWER · a CORINTHIAN temple ──────────────────────────────────────────
// A live view of the machine's two lifelines — its battery (UPower) and its link
// to the world (/proc/net/route). Third of the pantheon; it shares the marble-
// stele grammar but stands in its OWN order, under its OWN signature hue:
//
//   · ORDER      — CORINTHIAN, the ornate order. Its signature is MUREX (violet):
//                  the clef, the inset keyline, the box-drawing frame and the
//                  entablature frieze all run in murex, where the Conductor ran
//                  gold and the Meters ran aegean. The frieze is an EGG-AND-DART
//                  ovolo (eggs alternating with darts) — neither the Conductor's
//                  baton course nor the Meters' running wave.
//   · MUSIC      — the battery's charge is a Nerd Font battery glyph now
//                  (khoa, 2026-08-17: the old rest-notation 𝄽…𝅂/𝆑/𝄮 icons
//                  retired — ten nf-md steps emptier→fuller, alert while low,
//                  lightning-bolt while charging). The link keeps the
//                  SUSTAINED note 𝅗𝅥 while up, a rest 𝄽 when the line goes
//                  dead. A gold 𝄂 closes the score.
//   · TERMINAL   — box-drawing frames the panel; charge is an ASCII bar
//                  ⟦▓▓▓░░░⟧ recast with true fill colour; iface names sit in mono.
//   · KAOMOJI    — one mood face reads the machine's overall footing.
//
// SHARED pantheon bars: opaque marble body, 2px plum border, inset keyline, clef
// inscription, Canvas frieze, TUI frame, closing 𝄂, ONE laurel-paletteHot
// standout (the LIVE network link), gold ink for the numeric tallies. An HONEST
// empty state: "AC — no battery" on a desktop, never a faked 100%. Sub-hues each
// earn a job: charge fill = teal(wireCyan), link + detail = aegean(holoBlue),
// low/offline = terracotta(paletteUrgent). Murex is the temple. Colour flows
// from `notes` roles only; hard corners (radius 0) everywhere.
Item {
    id: gadget

    required property var notes            // palette roles
    property string routePath: "/proc/net/route"

    implicitWidth: 340
    implicitHeight: 268

    // type voices ──────────────────────────────────────────────────────────────
    readonly property string faceSerif: "Noto Serif"
    readonly property string faceMono:  "JetBrainsMono Nerd Font"
    readonly property string faceMusic: "Noto Music"

    // this temple's signature accent — MUREX (Corinthian)
    readonly property color signature: notes.violet

    readonly property int cells: 14
    readonly property real lowAt: 15

    function withA(cstr, a) {
        var c = Qt.darker(cstr, 1.0);
        return Qt.rgba(c.r, c.g, c.b, a);
    }

    // ── BATTERY (UPower) ─────────────────────────────────────────────────────
    readonly property var battDev: UPower.displayDevice
    readonly property bool battAvail: battDev && battDev.isLaptopBattery && battDev.isPresent
    readonly property int battPct: battDev ? Math.round(battDev.percentage) : 0
    readonly property bool battCharging: battDev && battDev.state === UPowerDeviceState.Charging
    readonly property bool battFull: battDev && battDev.state === UPowerDeviceState.FullyCharged
    readonly property bool battLow: battAvail && !battCharging && battPct <= gadget.lowAt

    // battery icon by charge — Nerd Font (nf-md) battery glyphs, ten steps
    // emptier→fuller; alert while low, lightning-bolt while charging
    function battIcon() {
        if (!battAvail) return "󰂑";                 // battery-unknown — mains, at ease
        if (battFull) return "󰁹";                   // battery (full)
        if (battCharging) return "󰂄";               // battery-charging
        if (battLow) return "󰂃";                    // battery-alert — running low
        var steps = ["󰁺", "󰁻", "󰁼", "󰁽", "󰁾", "󰁿", "󰂀", "󰂁", "󰂂", "󰁹"];  // 10%…100%
        var idx = Math.floor(battPct / 100 * (steps.length - 1));
        if (idx < 0) idx = 0;
        if (idx >= steps.length) idx = steps.length - 1;
        return steps[idx];
    }
    function battWord() {
        if (!battAvail) return "on mains";
        if (battFull) return "full";
        if (battCharging) return "charging";
        if (battLow) return "running low";
        return "discharging";
    }
    function battHue() {
        if (!battAvail) return notes.holoBlue;      // aegean — steady mains
        if (battLow) return notes.paletteUrgent;    // terracotta
        if (battCharging || battFull) return notes.paletteAccent;   // gold
        return notes.wireCyan;                      // teal — on its own reserves
    }
    function battTime() {
        if (!battDev || !battAvail) return "";
        var sec = battCharging ? battDev.timeToFull : battDev.timeToEmpty;
        if (!sec || sec <= 0) return "";
        var min = Math.round(sec / 60), h = Math.floor(min / 60), m = min % 60;
        return (battCharging ? "full in " : "left ")
             + (h > 0 ? h + "h" + (m < 10 ? "0" : "") + m : min + "m");
    }
    function barTrack() { var s = ""; for (var i = 0; i < gadget.cells; i++) s += "░"; return s; }
    function barFill(pct) {
        var n = Math.round((pct < 0 ? 0 : pct > 100 ? 100 : pct) / 100 * gadget.cells), s = "";
        for (var i = 0; i < gadget.cells; i++) s += (i < n) ? "▓" : " ";
        return s;
    }

    // ── NETWORK (/proc/net/route) ────────────────────────────────────────────
    property string netIface: ""
    property bool netUp: false

    function parseRoute() {
        try {
            var t = route.text();
            if (!t) { netUp = false; netIface = ""; return; }
            var lines = t.split("\n");
            for (var i = 1; i < lines.length; i++) {          // skip header row
                var f = lines[i].trim().split(/\s+/);
                if (f.length < 4) continue;
                if (f[1] === "00000000") {                    // the default route
                    netIface = f[0];
                    netUp = true;
                    return;
                }
            }
            netUp = false; netIface = "";
        } catch (e) { netUp = false; netIface = ""; }
    }
    function netKind() {
        var n = gadget.netIface;
        if (/^(wl|wlan|wlp)/.test(n)) return "wireless";
        if (/^(en|eth|eno|enp|ens)/.test(n)) return "ethernet";
        if (/^(tun|tap|wg|ppp)/.test(n)) return "tunnel";
        return n ? "link" : "";
    }
    function netGlyph() { return gadget.netUp ? "𝅗𝅥" : "𝄽"; }   // sustained note vs rest
    function netHue() { return gadget.netUp ? notes.holoBlue : notes.paletteUrgent; }

    // ── one mood face for the whole box ───────────────────────────────────────
    function kaomojiFor() {
        if (!gadget.netUp) return "(・_・;)";           // cut off from the world
        if (gadget.battLow) return "(￣ヘ￣;)";         // nervous, thirsty
        if (gadget.battCharging) return "(っ˘ω˘ς )";    // sipping happily
        return "( ´ ▽ ` )ﾉ";                            // at ease, plugged & online
    }

    FileView { id: route; path: gadget.routePath; blockLoading: true; printErrors: false }
    function sample() { route.reload(); parseRoute(); }
    Timer { interval: 2000; running: true; repeat: true; onTriggered: gadget.sample() }
    Component.onCompleted: sample()

    // cast shadow ─────────────────────────────────────────────────────────────
    Rectangle {
        anchors.fill: stele
        anchors.leftMargin: 4; anchors.topMargin: 5
        anchors.rightMargin: -4; anchors.bottomMargin: -5
        radius: 0
        color: gadget.withA(notes.paletteFg, 0.22)
    }

    Rectangle {
        id: stele
        anchors.fill: parent
        radius: 0
        color: notes.paletteBg
        border.color: notes.paletteFg
        border.width: 2

        Rectangle {                                   // inset keyline — MUREX (Corinthian)
            anchors.fill: parent; anchors.margins: 4
            radius: 0; color: "transparent"
            border.color: gadget.signature; border.width: 1
        }

        Item {
            id: content
            anchors.fill: parent
            anchors.margins: 13

            // ── ENTABLATURE ───────────────────────────────────────────────────
            Item {
                id: head
                anchors.top: parent.top
                width: parent.width
                height: 38

                Rectangle {
                    anchors.fill: parent; anchors.bottomMargin: 5
                    color: gadget.withA(notes.paletteFg, 0.05)
                }
                Text {
                    id: clef
                    anchors.left: parent.left
                    anchors.verticalCenter: parent.verticalCenter
                    anchors.verticalCenterOffset: -3
                    // ϟ (Greek koppa) — Zeus's thunderbolt: a Greek letter that
                    // reads as lightning, crowning the POWER temple in place of a
                    // clef. Serif face so the koppa renders (Noto Music lacks it).
                    text: "ϟ"; font.family: gadget.faceSerif; font.pixelSize: 33; font.bold: true
                    color: gadget.signature
                }
                Text {
                    anchors.left: clef.right; anchors.leftMargin: 8
                    anchors.verticalCenter: parent.verticalCenter
                    text: "POWER"
                    font.family: gadget.faceSerif; font.pixelSize: 19
                    font.weight: Font.DemiBold; font.letterSpacing: 4
                    color: notes.paletteFg
                }
                Text {
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    text: "[ upower ]"
                    font.family: gadget.faceMono; font.pixelSize: 11
                    color: gadget.withA(gadget.signature, 0.95)
                }
            }

            // ── Corinthian egg-and-dart frieze (ovolo), in murex ──────────────
            Canvas {
                id: frieze
                anchors.top: head.bottom; anchors.topMargin: 2
                width: parent.width; height: 13
                onWidthChanged: requestPaint()
                onPaint: {
                    var ctx = getContext("2d");
                    ctx.reset();
                    ctx.clearRect(0, 0, width, height);
                    ctx.strokeStyle = gadget.signature;
                    ctx.lineWidth = 1.4;
                    var u = height / 5;
                    var top = u * 0.5, bot = height - u * 0.5, mid = (top + bot) / 2;
                    // the two rails
                    ctx.beginPath();
                    ctx.moveTo(0, top); ctx.lineTo(width, top);
                    ctx.moveTo(0, bot); ctx.lineTo(width, bot);
                    ctx.stroke();
                    // eggs (circles) alternating with darts (V), the length of the rail
                    var period = height * 1.15, r = (bot - top) / 2 - 1;
                    for (var x = 0; x < width; x += period) {
                        var cx = x + period * 0.3;
                        ctx.beginPath();
                        ctx.arc(cx, mid, r, 0, 2 * Math.PI);
                        ctx.stroke();
                        var dx = x + period * 0.8;         // the dart between eggs
                        ctx.beginPath();
                        ctx.moveTo(dx - 2, top + 1);
                        ctx.lineTo(dx, bot - 1);
                        ctx.lineTo(dx + 2, top + 1);
                        ctx.stroke();
                    }
                }
            }

            // ── box-drawing top frame ─────────────────────────────────────────
            Item {
                id: topFrame
                anchors.top: frieze.bottom; anchors.topMargin: 8
                width: parent.width; height: 15

                Text {
                    id: tfL
                    anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter
                    text: "┌─┤ ♪ lifelines ├"
                    font.family: gadget.faceMono; font.pixelSize: 11
                    color: gadget.withA(gadget.signature, 0.95)
                }
                Text {
                    id: tfR
                    anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                    text: "┐"
                    font.family: gadget.faceMono; font.pixelSize: 11
                    color: gadget.withA(gadget.signature, 0.95)
                }
                Rectangle {
                    anchors.left: tfL.right; anchors.right: tfR.left
                    anchors.leftMargin: 2; anchors.rightMargin: 2
                    anchors.verticalCenter: parent.verticalCenter
                    anchors.verticalCenterOffset: 1
                    height: 1; color: gadget.withA(gadget.signature, 0.55)
                }
            }

            // ── box-drawing bottom frame ──────────────────────────────────────
            Item {
                id: footer
                anchors.bottom: parent.bottom
                width: parent.width; height: 20

                Text {
                    id: ffL
                    anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter
                    text: "└─┤ " + (gadget.battAvail ? gadget.battPct + "% batt" : "AC")
                          + " · " + (gadget.netUp ? gadget.netIface : "offline") + " ├"
                    font.family: gadget.faceMono; font.pixelSize: 11
                    color: gadget.withA(notes.paletteFg, 0.8)
                }
                Text {
                    id: ffCorner
                    anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                    text: "┘"
                    font.family: gadget.faceMono; font.pixelSize: 11
                    color: gadget.withA(gadget.signature, 0.95)
                }
                Text {                                // final barline — gold ink (shared)
                    id: ffBar
                    anchors.right: ffCorner.left; anchors.rightMargin: 4
                    anchors.verticalCenter: parent.verticalCenter
                    anchors.verticalCenterOffset: -1
                    text: "𝄂"
                    font.family: gadget.faceMusic; font.pixelSize: 18
                    color: notes.paletteAccent
                }
                Rectangle {
                    anchors.left: ffL.right; anchors.right: ffBar.left
                    anchors.leftMargin: 2; anchors.rightMargin: 6
                    anchors.verticalCenter: parent.verticalCenter
                    anchors.verticalCenterOffset: 1
                    height: 1; color: gadget.withA(gadget.signature, 0.55)
                }
            }

            // ── THE STAVE: battery voice + network voice + a mood face ────────
            Item {
                id: stave
                anchors.top: topFrame.bottom; anchors.topMargin: 6
                anchors.bottom: footer.top; anchors.bottomMargin: 2
                width: parent.width

                Column {
                    id: col
                    anchors.left: parent.left; anchors.right: parent.right
                    anchors.top: parent.top
                    spacing: 12

                    // ── BATTERY VOICE ─────────────────────────────────────────
                    Item {
                        width: parent.width; height: 46

                        Item {                                // battery glyph on a mono column
                            id: battGutter
                            anchors.left: parent.left; anchors.leftMargin: 4
                            anchors.top: parent.top
                            width: 30; height: 24
                            Text {
                                anchors.centerIn: parent
                                anchors.verticalCenterOffset: -2
                                text: gadget.battIcon()
                                font.family: gadget.faceMono; font.pixelSize: 22
                                color: gadget.battHue()
                            }
                        }

                        Row {
                            id: battCap
                            anchors.left: battGutter.right; anchors.leftMargin: 8
                            anchors.top: parent.top
                            spacing: 8
                            Text {
                                id: battName
                                text: "BATTERY"
                                font.family: gadget.faceSerif; font.pixelSize: 15
                                font.weight: Font.Medium; font.letterSpacing: 2
                                color: notes.paletteFg
                            }
                            Text {
                                anchors.baseline: battName.baseline
                                text: gadget.battWord()
                                font.family: gadget.faceSerif; font.italic: true
                                font.pixelSize: 11
                                color: gadget.battHue()
                            }
                        }
                        Text {                                // % (or AC) tallied in gold
                            anchors.right: parent.right; anchors.rightMargin: 12
                            anchors.verticalCenter: battCap.verticalCenter
                            text: gadget.battAvail ? gadget.battPct + "%" : "AC"
                            font.family: gadget.faceMono; font.pixelSize: 15
                            color: gadget.battLow ? notes.paletteUrgent : notes.paletteAccent
                        }

                        // gauge (present) OR honest "no battery" line (absent)
                        Item {
                            anchors.left: battGutter.right; anchors.leftMargin: 8
                            anchors.right: parent.right; anchors.rightMargin: 12
                            anchors.bottom: parent.bottom; anchors.bottomMargin: 2
                            height: 18

                            Text {                            // honest empty state
                                anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter
                                visible: !gadget.battAvail
                                text: "AC — no battery present"
                                font.family: gadget.faceSerif; font.italic: true
                                font.pixelSize: 12
                                color: gadget.withA(notes.holoBlue, 0.95)
                            }

                            // the charge gauge — ASCII bar recast, teal fill
                            Text {
                                id: bBraL
                                visible: gadget.battAvail
                                anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter
                                text: "⟦"
                                font.family: gadget.faceMono; font.pixelSize: 15
                                color: gadget.withA(gadget.signature, 0.9)
                            }
                            Text {
                                id: bBraR
                                visible: gadget.battAvail
                                anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                                text: "⟧"
                                font.family: gadget.faceMono; font.pixelSize: 15
                                color: gadget.withA(gadget.signature, 0.9)
                            }
                            Item {
                                visible: gadget.battAvail
                                anchors.left: bBraL.right; anchors.leftMargin: 2
                                anchors.right: bBraR.left; anchors.rightMargin: 2
                                anchors.verticalCenter: parent.verticalCenter
                                height: parent.height
                                Text {                        // track
                                    anchors.fill: parent
                                    verticalAlignment: Text.AlignVCenter
                                    text: gadget.barTrack()
                                    font.family: gadget.faceMono; font.pixelSize: 14
                                    fontSizeMode: Text.HorizontalFit
                                    color: gadget.withA(notes.paletteFg, 0.22)
                                }
                                Text {                        // fill — teal / gold / terracotta
                                    anchors.fill: parent
                                    verticalAlignment: Text.AlignVCenter
                                    text: gadget.barFill(gadget.battPct)
                                    font.family: gadget.faceMono; font.pixelSize: 14
                                    fontSizeMode: Text.HorizontalFit
                                    color: gadget.battLow ? notes.paletteUrgent
                                          : (gadget.battCharging || gadget.battFull)
                                            ? notes.paletteAccent : notes.wireCyan
                                }
                            }
                        }
                    }

                    // ── NETWORK VOICE (the single laurel standout when up) ────
                    Item {
                        id: netRow
                        width: parent.width; height: 46
                        readonly property bool emph: gadget.netUp

                        Rectangle {                          // laurel spine — a live link
                            anchors.left: parent.left; anchors.top: parent.top
                            anchors.bottom: parent.bottom
                            width: 3; color: notes.paletteHot; visible: netRow.emph
                        }
                        Rectangle {
                            anchors.fill: parent
                            color: netRow.emph ? gadget.withA(notes.paletteHot, 0.08) : "transparent"
                        }

                        Item {                                // sustained note / rest
                            id: netGutter
                            anchors.left: parent.left; anchors.leftMargin: 4
                            anchors.top: parent.top
                            width: 30; height: 24
                            Text {
                                anchors.centerIn: parent
                                anchors.verticalCenterOffset: -2
                                text: gadget.netGlyph()
                                font.family: gadget.faceMusic; font.pixelSize: 24
                                color: netRow.emph ? notes.paletteHot : gadget.netHue()
                            }
                        }

                        Row {
                            id: netCap
                            anchors.left: netGutter.right; anchors.leftMargin: 8
                            anchors.top: parent.top
                            spacing: 8
                            Text {
                                id: netName
                                text: "NETWORK"
                                font.family: gadget.faceSerif; font.pixelSize: 15
                                font.weight: netRow.emph ? Font.Bold : Font.Medium
                                font.letterSpacing: 2
                                color: notes.paletteFg
                            }
                            Text {
                                anchors.baseline: netName.baseline
                                text: gadget.netUp ? gadget.netKind() : "no route"
                                font.family: gadget.faceSerif; font.italic: true
                                font.pixelSize: 11
                                color: netRow.emph ? notes.paletteHot : gadget.netHue()
                            }
                        }
                        Text {                                // up / — in gold
                            anchors.right: parent.right; anchors.rightMargin: 12
                            anchors.verticalCenter: netCap.verticalCenter
                            text: gadget.netUp ? "up" : "—"
                            font.family: gadget.faceMono; font.pixelSize: 15
                            color: gadget.netUp ? notes.paletteAccent : notes.paletteUrgent
                        }

                        // link line — iface in mono, an aegean established-link bar
                        Item {
                            anchors.left: netGutter.right; anchors.leftMargin: 8
                            anchors.right: parent.right; anchors.rightMargin: 12
                            anchors.bottom: parent.bottom; anchors.bottomMargin: 2
                            height: 18
                            Text {
                                anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter
                                text: gadget.netUp ? ("⇅ " + gadget.netIface) : "⌀ link down"
                                font.family: gadget.faceMono; font.pixelSize: 12
                                color: gadget.netUp ? gadget.withA(notes.holoBlue, 0.95)
                                                    : gadget.withA(notes.paletteUrgent, 0.9)
                            }
                            Text {                            // established-link ticks (aegean)
                                anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                                visible: gadget.netUp
                                text: "▰▰▰▰▰"
                                font.family: gadget.faceMono; font.pixelSize: 12
                                color: gadget.withA(notes.holoBlue, 0.85)
                            }
                        }
                    }

                    // ── the mood face + battery time-remaining, on a ledger line ─
                    Item {
                        width: parent.width; height: 16
                        Rectangle {
                            anchors.top: parent.top
                            anchors.left: parent.left; anchors.leftMargin: 4
                            anchors.right: parent.right; anchors.rightMargin: 12
                            height: 1; color: gadget.withA(gadget.signature, 0.30)
                        }
                        Text {
                            anchors.left: parent.left; anchors.leftMargin: 4
                            anchors.bottom: parent.bottom
                            text: gadget.kaomojiFor()
                            font.pixelSize: 11
                            color: gadget.withA(gadget.netUp && !gadget.battLow
                                                ? notes.holoBlue : notes.paletteUrgent, 0.9)
                        }
                        Text {
                            anchors.right: parent.right; anchors.rightMargin: 12
                            anchors.bottom: parent.bottom
                            text: gadget.battAvail ? gadget.battTime() : "mains · steady"
                            font.family: gadget.faceMono; font.pixelSize: 10
                            color: gadget.withA(notes.holoBlue, 0.95)
                        }
                    }
                }
            }
        }
    }
}
