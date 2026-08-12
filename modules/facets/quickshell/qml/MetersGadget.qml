import QtQuick
import Quickshell
import Quickshell.Io

// ── THE METERS · a DORIC temple ──────────────────────────────────────────────
// A live view of the machine's two vital pressures — CPU and RAM — read straight
// from the kernel's own ledgers (/proc/stat, /proc/meminfo) on a ~2s tick.
// One of the pantheon; it shares the marble-stele grammar with THE CONDUCTOR but
// stands in its OWN order, under its OWN signature hue and its OWN clef — plainly
// a separate building from the Ionic/aegean Terminals temple, not a recolor:
//
//   · ORDER      — DORIC, the plain sturdy order. Its clef is the ALTO C-clef 𝄡
//                  (each temple gets its own); its signature is TEAL (wireCyan):
//                  the clef, the inset keyline, the box-drawing frame and the
//                  entablature frieze all run teal, where the Conductor ran gold.
//                  The frieze is a Doric TRIGLYPH-AND-METOPE band (grooved blocks
//                  spaced by flat panels) — not the Conductor's solid baton course
//                  nor the Terminals' Ionic running-wave.
//   · MUSIC      — each meter is a VOICE, its load spoken as a DYNAMIC marking
//                  (𝆏𝆏 pp · 𝆏 p · 𝆐 mf · 𝆑 f · 𝆑𝆑 ff): a quiet machine plays
//                  softly, a hammered one fortissimo. A gold 𝄂 closes the score.
//   · TERMINAL   — box-drawing frames the panel; each gauge is an ASCII bar
//                  ⟦▓▓▓▓▓░░░░░⟧ recast with true fill colour.
//   · KAOMOJI    — one mood face reads the overall pressure of the box.
//
// SHARED pantheon bars: opaque marble body, 2px plum border, inset keyline, a
// clef inscription, a Canvas frieze, TUI frame, closing 𝄂, ONE laurel-paletteHot
// standout (the calmest voice — silence is the healthy note), gold ink for the
// numeric tallies. Sub-hues each earn a job: CPU fill = murex(violet), RAM fill =
// teal(wireCyan), urgent ≥85% = terracotta(paletteUrgent). Aegean is the temple.
// All colour flows from `notes` roles; hard corners (radius 0) everywhere.
Item {
    id: gadget

    required property var notes            // palette roles
    property string statPath: "/proc/stat"
    property string memPath:  "/proc/meminfo"

    implicitWidth: 340
    implicitHeight: 268

    // type voices ──────────────────────────────────────────────────────────────
    readonly property string faceSerif: "Noto Serif"                // carved marble
    readonly property string faceMono:  "JetBrainsMono Nerd Font"   // the terminal
    readonly property string faceMusic: "Noto Music"                // notation

    // this temple's signature accent — TEAL (Doric), where Conductor ran gold
    // and Terminals runs aegean; keeps Meters a distinct building
    readonly property color signature: notes.wireCyan

    // live readings ──────────────────────────────────────────────────────────────
    property real cpuPct: 0
    property real ramPct: 0
    property real memUsedGb: 0
    property real memTotalGb: 0
    // CPU busy-time integration state (delta across samples)
    property real prevTotal: -1
    property real prevIdle: -1

    readonly property int cells: 14
    readonly property real urgentAt: 85

    function withA(cstr, a) {                      // alpha on a role string
        var c = Qt.darker(cstr, 1.0);
        return Qt.rgba(c.r, c.g, c.b, a);
    }
    function clampPct(p) { return p < 0 ? 0 : (p > 100 ? 100 : p); }

    // load → dynamic marking (the meter's musical voice) ─────────────────────────
    function dynamicFor(pct) {
        if (pct >= gadget.urgentAt) return "𝆑𝆑";   // fortissimo — hammered
        if (pct >= 70) return "𝆑";                 // forte
        if (pct >= 50) return "𝆐";                 // mezzo
        if (pct >= 25) return "𝆏";                 // piano
        return "𝆏𝆏";                               // pianissimo — barely a whisper
    }
    function loadLabel(pct) {
        if (pct >= gadget.urgentAt) return "strained";
        if (pct >= 70) return "busy";
        if (pct >= 50) return "working";
        if (pct >= 25) return "easy";
        return "at rest";
    }
    function filledCells(pct) { return Math.round(clampPct(pct) / 100 * gadget.cells); }
    function barTrack() {
        var s = ""; for (var i = 0; i < gadget.cells; i++) s += "░"; return s;
    }
    function barFill(pct) {
        var n = filledCells(pct), s = "";
        for (var i = 0; i < gadget.cells; i++) s += (i < n) ? "▓" : " ";
        return s;
    }

    // one mood face for the whole box, read off the hotter of the two voices ──────
    function kaomojiFor(peak) {
        if (peak >= gadget.urgentAt) return "(╬ಠ益ಠ)";   // furious — pegged
        if (peak >= 70) return "(￣ヘ￣;)";               // hauling
        if (peak >= 50) return "( -_-)ﾉ";                // steady
        if (peak >= 25) return "(｡･ω･｡)";                // content
        return "(￣ω￣) zzZ";                             // dozing, plenty spare
    }

    // which voice earns the single laurel: the calmest, if it's genuinely calm ────
    readonly property int emphVoice:                 // 1 = CPU, 2 = RAM, 0 = none
        (Math.min(cpuPct, ramPct) < 55)
            ? (cpuPct <= ramPct ? 1 : 2)
            : 0
    readonly property real peakPct: Math.max(cpuPct, ramPct)

    // ── the kernel ledgers, re-read on a tick (files, not processes) ─────────────
    function parseStat() {
        try {
            var t = stat.text();
            if (!t) return;
            var line = t.split("\n")[0];             // "cpu  u n s idle iowait ..."
            var f = line.trim().split(/\s+/);
            if (f[0] !== "cpu") return;
            var total = 0, idle = 0;
            for (var i = 1; i < f.length; i++) {
                var v = parseInt(f[i]);
                if (isNaN(v)) continue;
                total += v;
                if (i === 4 || i === 5) idle += v;   // idle + iowait
            }
            if (gadget.prevTotal >= 0) {
                var dt = total - gadget.prevTotal;
                var di = idle - gadget.prevIdle;
                if (dt > 0) gadget.cpuPct = gadget.clampPct((dt - di) / dt * 100);
            }
            gadget.prevTotal = total;
            gadget.prevIdle = idle;
        } catch (e) { /* keep last reading */ }
    }
    function parseMem() {
        try {
            var t = mem.text();
            if (!t) return;
            var kv = ({});
            var lines = t.split("\n");
            for (var i = 0; i < lines.length; i++) {
                var m = lines[i].match(/^(\w+):\s+(\d+)/);
                if (m) kv[m[1]] = parseInt(m[2]);    // kB
            }
            var tot = kv["MemTotal"] || 0;
            var avail = (kv["MemAvailable"] !== undefined)
                        ? kv["MemAvailable"]
                        : ((kv["MemFree"] || 0) + (kv["Buffers"] || 0) + (kv["Cached"] || 0));
            if (tot > 0) {
                var used = tot - avail;
                gadget.ramPct = gadget.clampPct(used / tot * 100);
                gadget.memUsedGb = used / 1048576;
                gadget.memTotalGb = tot / 1048576;
            }
        } catch (e) { /* keep last reading */ }
    }

    FileView { id: stat; path: gadget.statPath; blockLoading: true; printErrors: false }
    FileView { id: mem;  path: gadget.memPath;  blockLoading: true; printErrors: false }

    function sample() { stat.reload(); mem.reload(); parseStat(); parseMem(); }
    Timer { interval: 2000; running: true; repeat: true; onTriggered: gadget.sample() }
    Component.onCompleted: { sample(); }   // seed prevTotal; first cpu% lands next tick

    // cast shadow — lifts the stele off any marble wallpaper ─────────────────────
    Rectangle {
        anchors.fill: stele
        anchors.leftMargin: 4; anchors.topMargin: 5
        anchors.rightMargin: -4; anchors.bottomMargin: -5
        radius: 0
        color: gadget.withA(notes.paletteFg, 0.22)
    }

    // the stele ──────────────────────────────────────────────────────────────────
    Rectangle {
        id: stele
        anchors.fill: parent
        radius: 0
        color: notes.paletteBg
        border.color: notes.paletteFg
        border.width: 2

        Rectangle {                                   // inset keyline — AEGEAN (Ionic)
            anchors.fill: parent; anchors.margins: 4
            radius: 0; color: "transparent"
            border.color: gadget.signature; border.width: 1
        }

        Item {
            id: content
            anchors.fill: parent
            anchors.margins: 13

            // ── ENTABLATURE: clef · inscription · [proc] tag ──────────────────
            Item {
                id: head
                anchors.top: parent.top
                width: parent.width
                height: 38

                Rectangle {                           // deeper marble band
                    anchors.fill: parent; anchors.bottomMargin: 5
                    color: gadget.withA(notes.paletteFg, 0.05)
                }
                Text {
                    id: clef
                    anchors.left: parent.left
                    anchors.verticalCenter: parent.verticalCenter
                    anchors.verticalCenterOffset: -3
                    text: "𝄡"; font.family: gadget.faceMusic; font.pixelSize: 33  // alto C-clef
                    color: gadget.signature
                }
                Text {
                    anchors.left: clef.right; anchors.leftMargin: 8
                    anchors.verticalCenter: parent.verticalCenter
                    text: "METERS"
                    font.family: gadget.faceSerif; font.pixelSize: 19
                    font.weight: Font.DemiBold; font.letterSpacing: 4
                    color: notes.paletteFg
                }
                Text {                                // terminal tag
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    text: "[ /proc ]"
                    font.family: gadget.faceMono; font.pixelSize: 11
                    color: gadget.withA(gadget.signature, 0.95)
                }
            }

            // ── Doric triglyph-and-metope frieze, in teal ─────────────────────
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
                    var top = u * 0.5, bot = height - u * 0.5;
                    // the two rails (regula / taenia)
                    ctx.beginPath();
                    ctx.moveTo(0, top); ctx.lineTo(width, top);
                    ctx.moveTo(0, bot); ctx.lineTo(width, bot);
                    ctx.stroke();
                    // triglyph blocks (grooved) spaced by flat metope panels
                    var period = 22, block = 8;          // metope gap + triglyph width
                    ctx.beginPath();
                    for (var x = 2; x < width; x += period) {
                        // the block edges
                        ctx.moveTo(x,         top + 0.5); ctx.lineTo(x,         bot - 0.5);
                        ctx.moveTo(x + block, top + 0.5); ctx.lineTo(x + block, bot - 0.5);
                        // two inner grooves → the classic triglyph
                        ctx.moveTo(x + block / 3,     top + 1); ctx.lineTo(x + block / 3,     bot - 1);
                        ctx.moveTo(x + block * 2 / 3, top + 1); ctx.lineTo(x + block * 2 / 3, bot - 1);
                    }
                    ctx.stroke();
                }
            }

            // ── box-drawing top frame: ┌─┤ ♪ vitals ├────┐ ────────────────────
            Item {
                id: topFrame
                anchors.top: frieze.bottom; anchors.topMargin: 8
                width: parent.width; height: 15

                Text {
                    id: tfL
                    anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter
                    text: "┌─┤ ♪ vitals ├"
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

            // ── box-drawing bottom frame: └─┤ cpu · ram ├── 𝄂 ┘ ──────────────
            Item {
                id: footer
                anchors.bottom: parent.bottom
                width: parent.width; height: 20

                Text {
                    id: ffL
                    anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter
                    text: "└─┤ cpu " + Math.round(gadget.cpuPct) + "% · ram "
                          + Math.round(gadget.ramPct) + "% ├"
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

            // ── THE STAVE: two gauges + a mood face ───────────────────────────
            Item {
                id: stave
                anchors.top: topFrame.bottom; anchors.topMargin: 6
                anchors.bottom: footer.top; anchors.bottomMargin: 2
                width: parent.width

                Column {
                    id: gaugeColumn
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.top: parent.top
                    spacing: 12

                    // reusable gauge — invoked twice below
                    component Gauge : Item {
                        id: g
                        property string tag: "CPU"
                        property real pct: 0
                        property color fillHue: notes.violet
                        property bool emph: false
                        readonly property bool urgent: pct >= gadget.urgentAt
                        readonly property color liveHue: urgent ? notes.paletteUrgent
                                                               : (emph ? notes.paletteHot : fillHue)
                        width: parent ? parent.width : 0
                        height: 46

                        // laurel spine — the single standout (calmest voice)
                        Rectangle {
                            anchors.left: parent.left; anchors.top: parent.top
                            anchors.bottom: parent.bottom
                            width: 3; color: notes.paletteHot; visible: g.emph
                        }
                        Rectangle {
                            anchors.fill: parent
                            color: g.emph ? gadget.withA(notes.paletteHot, 0.08) : "transparent"
                        }

                        // top line: serif label · load word · dynamic marking · %
                        Row {
                            id: capRow
                            anchors.left: parent.left; anchors.leftMargin: 12
                            anchors.top: parent.top
                            spacing: 8
                            Text {
                                id: gName
                                text: g.tag
                                font.family: gadget.faceSerif; font.pixelSize: 15
                                font.weight: g.emph ? Font.Bold : Font.Medium
                                font.letterSpacing: 2
                                color: notes.paletteFg
                            }
                            Text {
                                anchors.baseline: gName.baseline
                                text: gadget.loadLabel(g.pct)
                                font.family: gadget.faceSerif; font.italic: true
                                font.pixelSize: 11
                                color: g.liveHue
                            }
                        }
                        Text {                                // dynamic marking (music)
                            anchors.right: gPct.left; anchors.rightMargin: 8
                            anchors.verticalCenter: capRow.verticalCenter
                            anchors.verticalCenterOffset: -3
                            text: gadget.dynamicFor(g.pct)
                            font.family: gadget.faceMusic; font.pixelSize: 20
                            color: g.liveHue
                        }
                        Text {                                // percent, tallied in gold
                            id: gPct
                            anchors.right: parent.right; anchors.rightMargin: 12
                            anchors.verticalCenter: capRow.verticalCenter
                            text: Math.round(g.pct) + "%"
                            font.family: gadget.faceMono; font.pixelSize: 15
                            color: g.urgent ? notes.paletteUrgent : notes.paletteAccent
                        }

                        // the gauge — ASCII bar ⟦▓▓▓░░░⟧ recast with true fill hue
                        Item {
                            anchors.left: parent.left; anchors.leftMargin: 12
                            anchors.right: parent.right; anchors.rightMargin: 12
                            anchors.bottom: parent.bottom; anchors.bottomMargin: 2
                            height: 18

                            Text {
                                id: braL
                                anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter
                                text: "⟦"
                                font.family: gadget.faceMono; font.pixelSize: 15
                                color: gadget.withA(gadget.signature, 0.9)
                            }
                            Text {
                                id: braR
                                anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                                text: "⟧"
                                font.family: gadget.faceMono; font.pixelSize: 15
                                color: gadget.withA(gadget.signature, 0.9)
                            }
                            Item {                            // the cell field
                                anchors.left: braL.right; anchors.leftMargin: 2
                                anchors.right: braR.left; anchors.rightMargin: 2
                                anchors.verticalCenter: parent.verticalCenter
                                height: parent.height
                                Text {                        // track (empty cells)
                                    anchors.fill: parent
                                    horizontalAlignment: Text.AlignLeft
                                    verticalAlignment: Text.AlignVCenter
                                    text: gadget.barTrack()
                                    font.family: gadget.faceMono; font.pixelSize: 14
                                    fontSizeMode: Text.HorizontalFit
                                    color: gadget.withA(notes.paletteFg, 0.22)
                                }
                                Text {                        // fill (▓ cells) in live hue
                                    anchors.fill: parent
                                    horizontalAlignment: Text.AlignLeft
                                    verticalAlignment: Text.AlignVCenter
                                    text: gadget.barFill(g.pct)
                                    font.family: gadget.faceMono; font.pixelSize: 14
                                    fontSizeMode: Text.HorizontalFit
                                    color: g.liveHue
                                }
                            }
                        }
                    }

                    Gauge {
                        tag: "CPU"; pct: gadget.cpuPct
                        fillHue: notes.violet                // murex
                        emph: gadget.emphVoice === 1
                    }
                    Gauge {
                        tag: "RAM"; pct: gadget.ramPct
                        fillHue: notes.holoBlue              // aegean
                        emph: gadget.emphVoice === 2
                    }

                    // the mood face + honest bytes, on the closing ledger line
                    Item {
                        width: parent.width; height: 16
                        Rectangle {
                            anchors.top: parent.top
                            anchors.left: parent.left; anchors.leftMargin: 12
                            anchors.right: parent.right; anchors.rightMargin: 12
                            height: 1; color: gadget.withA(gadget.signature, 0.30)
                        }
                        Text {
                            anchors.left: parent.left; anchors.leftMargin: 12
                            anchors.bottom: parent.bottom
                            text: gadget.kaomojiFor(gadget.peakPct)
                            font.pixelSize: 11
                            color: gadget.withA(gadget.peakPct >= gadget.urgentAt
                                                ? notes.paletteUrgent : gadget.signature, 0.9)
                        }
                        Text {                               // aegean detail — the honest bytes
                            anchors.right: parent.right; anchors.rightMargin: 12
                            anchors.bottom: parent.bottom
                            text: gadget.memUsedGb.toFixed(1) + " / "
                                  + gadget.memTotalGb.toFixed(0) + " GB"
                            font.family: gadget.faceMono; font.pixelSize: 10
                            color: gadget.withA(gadget.signature, 0.95)
                        }
                    }
                }
            }
        }
    }
}
