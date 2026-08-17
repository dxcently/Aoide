// herald-center.qml — sonata's "herald-center" slot: the notification CENTER,
// a Tuscan stele in the dock. The herald's SECOND form: since 2026-08-16 dunst
// owns org.freedesktop.Notifications and draws the popups themselves (the
// aoide.dunst dendrite); this widget is what remains on the Quickshell side —
// the standing ledger of what was heard, polled out of `dunstctl history`
// (JSON) every 5s.
//
// khoa, 2026-08-16 (second pass): the first center draft invented a leaner
// row grammar; khoa preferred the RETIRED popup card's design, so every
// entry below is that card, verbatim grammar at ledger scale — the box-
// drawing top frame with the program name in its label slot, the plain
// cleave rule, the bold serif title over the hairline-indented context, the
// ledger line (urgency kaomoji left, arrival clock right in gold), and the
// closing └─┤ urgency ├ … 𝄂 ┘ barline frame, all wrapped in the one outer
// stele carrying the ❧ / H E R A L D entablature and the Tuscan double-rule
// frieze. Critical entries keep the old card's two overrides: terracotta
// ink throughout, and the breathing opacity pulse.
//
// khoa, 2026-08-17 (third pass): the second pass had quietly shrunk the
// middle band (5px cleave, 13px title, 8px/2px context indent, 16px
// ledger) and the card-feel went with it — the ledger's kaomoji struck
// through its own rule. Restored the retired card's exact metrics: 9px
// cleave, 14px bold serif title with 2px top pad, context hairline with
// the 10px indent / 5px drop at 12px, 18px ledger line. Only the icon
// box (below) deviates from the old card, and it was khoa's own ask.
//
// khoa, 2026-08-16 (icons RE-admitted): the retired card explicitly dropped
// sender icons (its header tells that story); the center brings them back —
// each entry shows the app's PROVIDED icon/image: dunst's `icon_path` leaf
// first (an absolute path, or an icon name resolved through the theme via
// Quickshell.iconPath), then a theme lookup by desktop_entry / appname, then
// the ❧ crown glyph as the fallback mark. Only the icon IMAGE is trusted
// from the payload, and only ever as an Image source — house rule 4: all
// other history fields are rendered strictly as TEXT (no eval, no command
// construction, Text.PlainText throughout).
//
// Fixed slot contract: `notes`/`bridge` always (slots.md) — bridge goes
// unused today, declared per the contract. NO `notification` extra anymore:
// the data source is the daemon's history, not a server model.
//
// dunstctl history shape (dunst ≥1.9): {"data":[[ {…}, … ]]} — one inner
// array, oldest first, every leaf wrapped as {"type":…,"data":…}. Leaves we
// read: appname, summary, body, urgency ("LOW"/"NORMAL"/"CRITICAL"),
// timestamp (µs since epoch — converted defensively), desktop_entry,
// icon_path. Anything absent or unparseable degrades to the resting
// whisper, never an error.

import QtQuick
import Quickshell
import Quickshell.Io

Item {
    id: root

    required property var notes
    required property var bridge         // unused today — fixed slot contract

    readonly property color signature: root.notes.base0F   // rust (Tuscan)
    readonly property color urgent: root.notes.notifUrgent

    readonly property string faceSerif: "Noto Serif"
    readonly property string faceMono:  "JetBrainsMono Nerd Font"

    function withA(cstr, a) {
        var c = Qt.color(cstr)
        return Qt.rgba(c.r, c.g, c.b, a)
    }

    // ── The heard ledger ───────────────────────────────────────────────────
    property var entries: []
    property bool heardOnce: false   // dunstctl has answered at least once

    function leaf(v) {
        return (v !== null && typeof v === "object" && v.data !== undefined) ? v.data : v
    }
    function toMs(t) {
        t = Number(t) || 0
        if (t > 1e14) return t / 1000     // µs → ms
        if (t > 1e12) return t            // already ms
        return t * 1000                   // s → ms
    }
    function parseHistory(t) {
        if (!t || ("" + t).trim().length === 0) return
        var doc
        try { doc = JSON.parse(t) } catch (e) { return }
        var inner = doc && doc.data && doc.data[0]
        if (!inner || !inner.length) { root.entries = []; root.heardOnce = true; return }
        var out = []
        var start = Math.max(0, inner.length - 20)
        for (var i = inner.length - 1; i >= start; i--) {   // newest first
            var n = inner[i]
            out.push({
                app: ("" + (leaf(n.appname) || "notice")).trim(),
                entry: ("" + (leaf(n.desktop_entry) || "")).trim(),
                icon: ("" + (leaf(n.icon_path) || "")).trim(),
                summary: ("" + (leaf(n.summary) || "")).trim(),
                body: ("" + (leaf(n.body) || leaf(n.message) || "")).trim(),
                urgency: ("" + leaf(n.urgency)).toUpperCase(),
                atMs: root.toMs(leaf(n.timestamp))
            })
        }
        root.entries = out
        root.heardOnce = true
    }

    Process {
        id: histProc
        command: ["dunstctl", "history"]
        stdout: StdioCollector {
            id: histOut
            onStreamFinished: root.parseHistory(histOut.text)
        }
    }
    Timer {
        interval: 5000
        repeat: true
        running: true
        triggeredOnStart: true
        onTriggered: if (!histProc.running) histProc.running = true
    }

    // ── Smart payload reading (ported from the retired popup card) ─────────
    // Senders arrive in two shapes:
    //   1. Proper freedesktop: appName + summary (title) + body (message).
    //   2. Terminal-forwarded OSC-9: the WHOLE "Title: message" string lands
    //      in `summary`, `body` is empty, and appName is the forwarder, not
    //      the real program.
    // parentTitle(): desktop_entry first (".desktop" trimmed), then appname,
    // then the "notice" floor. titleOf()/contextOf(): when the body is
    // empty, split a "Title: message" summary at its first ": " so the
    // notif's own title and its message render as two differentiated tiers.
    function strippedEntry(e) {
        var s = (e || "").trim()
        if (s.slice(-8) === ".desktop") s = s.slice(0, -8)
        return s
    }
    function parentTitle(e) {
        var d = root.strippedEntry(e.entry)
        if (d.length > 0) return d.length > 28 ? d.slice(0, 27) + "…" : d
        var a = e.app
        if (a.length > 0) return a.length > 28 ? a.slice(0, 27) + "…" : a
        return "notice"
    }
    function splitIdx(e) {
        var s = e.summary
        if (s.length === 0 || e.body.length > 0) return -1
        var idx = s.indexOf(": ")
        if (idx <= 5 || idx >= 60) return -1 // too short/too long to be a title
        if (s.slice(0, idx).indexOf(" ") <= 0) return -1 // single word — not a title
        return idx
    }
    function titleOf(e) {
        var idx = root.splitIdx(e)
        return idx > 0 ? e.summary.slice(0, idx) : e.summary
    }
    function contextOf(e) {
        if (e.body.length > 0) return e.body
        var idx = root.splitIdx(e)
        return idx > 0 ? e.summary.slice(idx + 2) : ""
    }

    // ── Icon resolution — the app's PROVIDED icon/image ────────────────────
    // icon_path first: absolute path straight, bare name through the theme.
    // Then theme lookups by desktop_entry, then appname. Empty = no icon
    // resolved; the delegate falls back to the ❧ crown glyph (the OLD
    // herald's mark). Every theme lookup passes check=true so a MISSING icon
    // resolves to "" (→ ❧) instead of the image://icon/ checkerboard
    // placeholder iconPath otherwise always returns (khoa, 2026-08-17).
    function iconSource(e) {
        var p = e.icon
        if (p.length > 0) {
            if (p.charAt(0) === "/") return "file://" + p
            var r = Quickshell.iconPath(p, true)
            if (r.length > 0) return r
        }
        var d = root.strippedEntry(e.entry)
        if (d.length > 0) {
            var rd = Quickshell.iconPath(d, true)
            if (rd.length > 0) return rd
        }
        if (e.app.length > 0) {
            var ra = Quickshell.iconPath(e.app.toLowerCase(), true)
            if (ra.length > 0) return ra
        }
        return ""
    }

    // Same kana/punctuation vocabulary the retired card proved safe (no
    // Thai/Hangul — tofu-adjacent junk in live testing, its note said).
    function kaomojiFor(urg) {
        if (urg === "CRITICAL") return "(ノ｀ｏ´)ノ"   // alarmed — a critical herald
        if (urg === "LOW") return "(´ω｀)"            // at ease, unhurried
        return "( ・ω・)ノ"                           // attentive, on the case
    }
    function urgencyWord(urg) {
        if (urg === "CRITICAL") return "critical"
        if (urg === "LOW") return "low"
        return "normal"
    }
    // arrival clock — the ledger's right-hand ink (the entry's own stored
    // timestamp, not "now": this is a history, the time it WAS heard)
    function clockOf(ms) {
        if (ms <= 0) return "--:--"
        var d = new Date(ms)
        var h = d.getHours(), m = d.getMinutes()
        return (h < 10 ? "0" + h : h) + ":" + (m < 10 ? "0" + m : m)
    }

    width: parent ? parent.width : 360
    implicitHeight: stele.height + 5   // +5 clears the cast shadow's overhang

    // cast shadow — shared pantheon idiom ───────────────────────────────────
    Rectangle {
        anchors.fill: stele
        anchors.leftMargin: 4; anchors.topMargin: 5
        anchors.rightMargin: -4; anchors.bottomMargin: -5
        radius: 0
        color: root.withA(root.notes.paletteFg, 0.22)
    }

    Rectangle {
        id: stele
        width: parent.width
        anchors.top: parent.top
        radius: 0
        color: root.notes.notifBg
        border.color: root.notes.paletteFg
        border.width: 2
        height: content.implicitHeight + 20

        // inset keyline — the herald's rust signature
        Rectangle {
            anchors.fill: parent; anchors.margins: 4
            radius: 0; color: "transparent"
            border.color: root.signature; border.width: 1
        }

        Column {
            id: content
            anchors { left: parent.left; right: parent.right; top: parent.top }
            anchors.margins: 10
            spacing: 4

            // ── ENTABLATURE: crown + carved name + tally ─────────────────
            Item {
                width: parent.width
                height: 24

                Text {
                    anchors.left: parent.left
                    anchors.verticalCenter: parent.verticalCenter
                    text: "❧"
                    // Pinned, not faceSerif: Noto Serif has no U+2767, so this
                    // rode font fallback. Libertine's open fleuron (regular —
                    // bold smears it) is the crown khoa picked from a rendered
                    // lineup (2026-08-17); the popup's dunstrc pins the same.
                    font.family: "Linux Libertine O"
                    font.pixelSize: 20
                    color: root.signature
                }
                Text {
                    anchors.centerIn: parent
                    text: "H E R A L D"
                    font.family: root.faceSerif
                    font.pixelSize: 14
                    font.weight: Font.DemiBold
                    font.letterSpacing: 3
                    color: root.notes.paletteFg
                }
                Text {                           // the tally — gold ink
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    text: root.entries.length + " heard"
                    font.family: root.faceMono
                    font.pixelSize: 10
                    color: root.withA(root.notes.paletteAccent, 0.95)
                }
            }

            // ── Tuscan frieze — a PLAIN double rule, deliberately un-
            // ornamented (that bareness IS the Tuscan order) ──────────────
            Item {
                width: parent.width
                height: 5
                Rectangle { width: parent.width; height: 1; y: 0; color: root.withA(root.signature, 0.7) }
                Rectangle { width: parent.width; height: 1; y: 4; color: root.withA(root.signature, 0.35) }
            }

            // ── the resting whisper (empty / dunst absent) ───────────────
            Text {
                visible: root.entries.length === 0
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                text: root.heardOnce ? "the herald rests — nothing heard"
                                     : "the herald is deaf (dunst not answering)"
                font.family: root.faceSerif
                font.italic: true
                font.pixelSize: 11
                color: root.withA(root.notes.paletteFg, 0.45)
                topPadding: 6; bottomPadding: 6
            }

            // ── the heard entries — newest first, each wearing the retired
            // popup card's full frame grammar at ledger scale ─────────────
            Repeater {
                model: root.entries
                delegate: Item {
                    id: entry
                    width: parent.width
                    height: entryCol.implicitHeight + 8   // breath between steles

                    readonly property bool critical: modelData.urgency === "CRITICAL"
                    readonly property color ink: entry.critical ? root.urgent : root.signature
                    readonly property string iconSrc: root.iconSource(modelData)

                    // Critical breathes — the terracotta-summons pulse idiom
                    // the retired popup card wore, so a critical line still
                    // reads as LIVE in the standing ledger.
                    SequentialAnimation on opacity {
                        running: entry.critical
                        loops: Animation.Infinite
                        NumberAnimation { to: 0.8; duration: 700; easing.type: Easing.InOutQuad }
                        NumberAnimation { to: 1.0; duration: 700; easing.type: Easing.InOutQuad }
                    }

                    Column {
                        id: entryCol
                        anchors { left: parent.left; right: parent.right; top: parent.top }
                        spacing: 4

                        // ── box-drawing top frame — app name in the label ─
                        Item {
                            width: parent.width
                            height: 15
                            Text {
                                id: tfL
                                anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter
                                text: "┌─┤ " + root.parentTitle(modelData) + " ├"
                                font.family: root.faceMono; font.pixelSize: 11
                                color: root.withA(entry.ink, 0.95)
                            }
                            Text {
                                id: tfR
                                anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                                text: "┐"
                                font.family: root.faceMono; font.pixelSize: 11
                                color: root.withA(entry.ink, 0.95)
                            }
                            Rectangle {
                                anchors.left: tfL.right; anchors.right: tfR.left
                                anchors.leftMargin: 1; anchors.rightMargin: 1
                                anchors.verticalCenter: parent.verticalCenter
                                height: 1; color: root.withA(entry.ink, 0.8)
                            }
                        }

                        // ── the cleave — one plain rule between the program
                        // frame and the message, the entablature/shaft line ─
                        Item {
                            width: parent.width
                            height: 9
                            Rectangle {
                                anchors.top: parent.top; anchors.topMargin: 1
                                width: parent.width; height: 1
                                color: root.withA(entry.ink, 0.55)
                            }
                        }

                        // ── icon + title / context — the sender's PROVIDED
                        // icon (or themed lookup, or the ❧ fallback) beside
                        // two differentiated text tiers ────────────────────
                        Item {
                            width: parent.width
                            visible: titleLabel.text.length > 0 || contextLabel.text.length > 0
                            height: Math.max(iconBox.height, words.implicitHeight)

                            Item {
                                id: iconBox
                                anchors.left: parent.left; anchors.top: parent.top
                                width: 26; height: 26
                                visible: titleLabel.text.length > 0 || contextLabel.text.length > 0
                                Image {
                                    id: iconImg
                                    anchors.fill: parent
                                    visible: entry.iconSrc !== "" && status === Image.Ready
                                    source: entry.iconSrc
                                    fillMode: Image.PreserveAspectFit
                                    mipmap: true
                                    asynchronous: true
                                }
                                Text {           // no icon resolved (or it failed to load) — the crown glyph stands in
                                    anchors.centerIn: parent
                                    visible: !iconImg.visible
                                    text: "❧"
                                    font.family: "Linux Libertine O"   // pinned with the entablature crown
                                    font.pixelSize: 16
                                    color: root.withA(entry.ink, 0.8)
                                }
                            }
                            Column {
                                id: words
                                anchors.left: iconBox.right; anchors.leftMargin: 8
                                anchors.right: parent.right
                                spacing: 4
                                Text {
                                    id: titleLabel
                                    width: parent.width
                                    text: root.titleOf(modelData)
                                    textFormat: Text.PlainText
                                    visible: text.length > 0
                                    color: entry.critical ? root.urgent : root.notes.notifFg
                                    font.family: root.faceSerif
                                    font.bold: true
                                    font.pixelSize: 14
                                    wrapMode: Text.WordWrap
                                    topPadding: 2
                                }
                                Item {
                                    width: parent.width
                                    visible: contextLabel.text.length > 0
                                    height: contextLabel.implicitHeight + 5
                                    Rectangle {           // the faint signature hairline
                                        anchors.left: parent.left
                                        anchors.top: contextLabel.top
                                        anchors.bottom: contextLabel.bottom
                                        width: 1
                                        color: root.withA(entry.ink, 0.45)
                                    }
                                    Text {
                                        id: contextLabel
                                        anchors.top: parent.top; anchors.topMargin: 5
                                        anchors.left: parent.left; anchors.leftMargin: 10
                                        anchors.right: parent.right
                                        text: root.contextOf(modelData)
                                        textFormat: Text.PlainText
                                        color: root.notes.notifFg
                                        opacity: 0.85
                                        font.pixelSize: 12
                                        lineHeight: 1.3
                                        wrapMode: Text.WordWrap
                                    }
                                }
                            }
                        }

                        // ── ledger line: urgency kaomoji left, arrival
                        // clock right in gold (terracotta while critical) ──
                        Item {
                            width: parent.width
                            height: 18
                            Rectangle {
                                anchors.top: parent.top
                                anchors.left: parent.left; anchors.right: parent.right
                                height: 1; color: root.withA(entry.ink, 0.3)
                            }
                            Text {
                                anchors.left: parent.left
                                anchors.bottom: parent.bottom
                                text: root.kaomojiFor(modelData.urgency)
                                font.family: root.faceMono   // Nerd Font cells — MoodFaces' proven kaomoji metrics
                                font.pixelSize: 11
                                color: root.withA(entry.ink, 0.9)
                            }
                            Text {
                                anchors.right: parent.right
                                anchors.bottom: parent.bottom
                                text: root.clockOf(modelData.atMs)
                                font.family: root.faceMono
                                font.pixelSize: 10
                                color: entry.critical ? root.urgent : root.notes.paletteAccent
                            }
                        }

                        // ── box-drawing bottom frame — urgency word in the
                        // label, closing 𝄂 barline in gold ──────────────────
                        Item {
                            width: parent.width
                            height: 18
                            Text {
                                id: ffL
                                anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter
                                text: "└─┤ " + root.urgencyWord(modelData.urgency) + " ├"
                                font.family: root.faceMono; font.pixelSize: 11
                                color: root.withA(root.notes.notifFg, 0.8)
                            }
                            Text {
                                id: ffCorner
                                anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                                text: "┘"
                                font.family: root.faceMono; font.pixelSize: 11
                                color: root.withA(entry.ink, 0.95)
                            }
                            Text {
                                id: ffBar
                                anchors.right: ffCorner.left; anchors.rightMargin: 4
                                anchors.verticalCenter: parent.verticalCenter
                                text: "𝄂"
                                font.family: "Noto Music"; font.pixelSize: 16
                                color: root.notes.paletteAccent
                            }
                            Rectangle {
                                anchors.left: ffL.right; anchors.right: ffBar.left
                                anchors.leftMargin: 2; anchors.rightMargin: 6
                                anchors.verticalCenter: parent.verticalCenter
                                height: 1; color: root.withA(entry.ink, 0.55)
                            }
                        }
                    }
                }
            }
        }
    }
}
