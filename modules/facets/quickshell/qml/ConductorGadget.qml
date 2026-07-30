import QtQuick
import Quickshell
import Quickshell.Io

// ── THE CONDUCTOR ────────────────────────────────────────────────────────────
// A live view of the agent-session roster (the `aoide baton` conductor):
// a Doric temple rendered in a terminal, keeping a musical score.
//
// Four voices, layered so each earns its place:
//   · GREEK      — an opaque marble stele: plum-ink border, inset Attic-gold
//                  keyline, a treble clef + carved serif inscription, and a
//                  drawn Greek-key meander ruling off the entablature.
//   · MUSIC      — the roster is a stave; each session is a note on a ledger
//                  line; its live state is spoken as a notation glyph
//                  (♪ working · 𝄐 awaiting · 𝄽 idle · 𝄂 done · · unknown), and
//                  a final barline 𝄂 closes the score. (glyphs = HARD CONTRACT)
//   · TERMINAL   — box-drawing frames the roster like a TUI panel; each row
//                  hangs from a monospace column │ (pilaster + staff barline).
//   · KAOMOJI    — a small mood face gives each session life; the empty stage
//                  gets an ASCII temple and a shrug.
//
// LEGIBLE + DEFINED: the body is opaque marble with a hard 2px plum border and
// a gold keyline — no pale washout. RESTRAINED: motifs carry state or structure.
// All colour flows from `notes` roles; hard corners (radius 0) everywhere.
Item {
    id: gadget

    required property var notes            // palette roles
    required property var bridge           // socket sender (bridge.focusSession)
    property var shared: null              // cross-widget state (shared.tracedSessionId)
    property string stagePath: "/home/khoa/Aoide/song/stage/sessions.json"

    implicitWidth: 360
    implicitHeight: 520

    // type voices ──────────────────────────────────────────────────────────────
    readonly property string faceSerif: "Noto Serif"                // carved marble
    readonly property string faceMono:  "JetBrainsMono Nerd Font"   // the terminal
    readonly property string faceMusic: "Noto Music"                // notation

    // live state ────────────────────────────────────────────────────────────────
    property var sessions: []
    property real nowMs: Date.now()
    property string emphId: ""
    property int projectCount: 0

    function withA(cstr, a) {                      // alpha on a role string
        var c = Qt.darker(cstr, 1.0);
        return Qt.rgba(c.r, c.g, c.b, a);
    }

    // state → notation glyph (VERBATIM contract, from baton/theme.rs) ────────────
    function glyphFor(state) {
        var s = (state || "").toLowerCase();
        if (/(run|active|tool|work|busy|trace)/.test(s)) return "♪";   // working
        if (/(await|block|notif|input|wait)/.test(s))    return "𝄐";   // awaiting
        if (/(idle|ready|sleep)/.test(s))                return "𝄽";   // idle
        if (/(done|stop|exit|finish|complete)/.test(s))  return "𝄂";   // done/stop
        return "·";                                                     // unknown
    }
    function isAwaiting(state) {
        return /(await|block|notif|input|wait)/.test((state || "").toLowerCase());
    }
    function stateColor(state) {
        var s = (state || "").toLowerCase();
        if (/(await|block|notif|input|wait)/.test(s))    return notes.paletteUrgent;
        if (/(run|active|tool|work|busy|trace)/.test(s)) return notes.paletteAccent;
        if (/(idle|ready|sleep)/.test(s))                return notes.violet;
        if (/(done|stop|exit|finish|complete)/.test(s))  return notes.wireCyan;
        return withA(notes.paletteFg, 0.45);
    }
    function stateLabel(state) {
        var s = (state || "").toLowerCase();
        if (/(await|block|notif|input|wait)/.test(s))    return "awaiting";
        if (/(run|active|tool|work|busy|trace)/.test(s)) return "working";
        if (/(idle|ready|sleep)/.test(s))                return "idle";
        if (/(done|stop|exit|finish|complete)/.test(s))  return "done";
        return "—";
    }
    // a mood face — the emoticon life ────────────────────────────────────────────
    function kaomojiFor(state) {
        var s = (state || "").toLowerCase();
        if (/(await|block|notif|input|wait)/.test(s))    return "(；・∀・)";   // anxious, waiting
        if (/(run|active|tool|work|busy|trace)/.test(s)) return "♪(´ε｀ )";    // humming along
        if (/(idle|ready|sleep)/.test(s))                return "(－ω－) zzZ";  // dozing
        if (/(done|stop|exit|finish|complete)/.test(s))  return "( ´ ▽ ` )";  // content, retired
        return "( ・_・)";                                                       // puzzled
    }

    // the single emphasized session (traced, else first working) ─────────────────
    function computeEmph() {
        var i;
        if (shared && shared.tracedSessionId) {
            for (i = 0; i < sessions.length; i++)
                if (sessions[i].sessionId === shared.tracedSessionId)
                    return shared.tracedSessionId;
        }
        for (i = 0; i < sessions.length; i++)
            if (/(run|active|tool|work|busy|trace)/.test((sessions[i].state || "").toLowerCase()))
                return sessions[i].sessionId;
        return "";
    }

    function elapsed(startedAt) {
        if (!startedAt) return "";
        var t = Date.parse(startedAt);
        if (isNaN(t)) return "";
        var d = Math.max(0, Math.floor((nowMs - t) / 1000));
        if (d < 60) return d + "s";
        var m = Math.floor(d / 60), s = d % 60;
        if (m < 60) return m + "m" + (s < 10 ? "0" : "") + s + "s";
        var h = Math.floor(m / 60), m2 = m % 60;
        return h + "h" + (m2 < 10 ? "0" : "") + m2 + "m";
    }
    function shortCwd(p) {
        if (!p) return "";
        p = p.replace(/^\/home\/[^/]+/, "~");
        if (p === "~") return "~";
        var parts = p.split("/").filter(function (x) { return x.length; });
        if (parts.length <= 2) return p;
        return "…/" + parts.slice(-2).join("/");
    }

    function recompute() {
        emphId = computeEmph();
        var seen = ({}), n = 0;
        for (var i = 0; i < sessions.length; i++) {
            var c = sessions[i].cwd || "?";
            if (!seen[c]) { seen[c] = true; n++; }
        }
        projectCount = n;
    }
    function parseStage() {
        try {
            var t = stage.text();
            if (!t || t.trim().length === 0) { sessions = []; recompute(); return; }
            var o = JSON.parse(t);
            sessions = (o && o.sessions) ? o.sessions : [];
        } catch (e) {
            sessions = [];
        }
        recompute();
    }
    onSessionsChanged: recompute()

    // the stage file — hot-reloads on change ─────────────────────────────────────
    FileView {
        id: stage
        path: gadget.stagePath
        watchChanges: true
        blockLoading: false
        printErrors: false
        onLoaded: gadget.parseStage()
        onFileChanged: reload()
    }
    Timer { interval: 1000; running: true; repeat: true; onTriggered: gadget.nowMs = Date.now() }
    Component.onCompleted: parseStage()

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

        Rectangle {                                   // inset gold keyline
            anchors.fill: parent; anchors.margins: 4
            radius: 0; color: "transparent"
            border.color: notes.paletteAccent; border.width: 1
        }

        Item {
            id: content
            anchors.fill: parent
            anchors.margins: 13

            // ── ENTABLATURE: clef · inscription · [baton] tag ─────────────────
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
                    text: "𝄞"; font.family: gadget.faceMusic; font.pixelSize: 33
                    color: notes.paletteAccent
                }
                Text {
                    anchors.left: clef.right; anchors.leftMargin: 8
                    anchors.verticalCenter: parent.verticalCenter
                    text: "CONDUCTOR"
                    font.family: gadget.faceSerif; font.pixelSize: 19
                    font.weight: Font.DemiBold; font.letterSpacing: 4
                    color: notes.paletteFg
                }
                Text {                                // terminal tag
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    text: "[ baton ]"
                    font.family: gadget.faceMono; font.pixelSize: 11
                    color: gadget.withA(notes.wireCyan, 0.95)
                }
            }

            // ── Greek-key meander (the staff rule) ────────────────────────────
            Canvas {
                id: meander
                anchors.top: head.bottom; anchors.topMargin: 2
                width: parent.width; height: 13
                onWidthChanged: requestPaint()
                onPaint: {
                    var ctx = getContext("2d");
                    ctx.reset();
                    ctx.clearRect(0, 0, width, height);
                    ctx.strokeStyle = notes.paletteAccent;
                    ctx.lineWidth = 1.4;
                    var u = height / 5;
                    var top = u * 0.5, bot = height - u * 0.5, period = u * 6;
                    ctx.beginPath();
                    ctx.moveTo(0, top); ctx.lineTo(width, top);
                    ctx.moveTo(0, bot); ctx.lineTo(width, bot);
                    ctx.stroke();
                    ctx.beginPath();
                    for (var x = 0; x < width; x += period) {
                        ctx.moveTo(x + u,     top);
                        ctx.lineTo(x + u,     bot - u);
                        ctx.lineTo(x + u * 4, bot - u);
                        ctx.lineTo(x + u * 4, top + u);
                        ctx.lineTo(x + u * 2, top + u);
                        ctx.lineTo(x + u * 2, bot - u * 2);
                        ctx.lineTo(x + u * 3, bot - u * 2);
                    }
                    ctx.stroke();
                }
            }

            // ── box-drawing top frame: ┌─┤ label ├────┐ ───────────────────────
            Item {
                id: topFrame
                anchors.top: meander.bottom; anchors.topMargin: 8
                width: parent.width; height: 15

                Text {
                    id: tfL
                    anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter
                    text: "┌─┤ ♪ live roster ├"
                    font.family: gadget.faceMono; font.pixelSize: 11
                    color: gadget.withA(notes.paletteAccent, 0.95)
                }
                Text {
                    id: tfR
                    anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                    text: "┐"
                    font.family: gadget.faceMono; font.pixelSize: 11
                    color: gadget.withA(notes.paletteAccent, 0.95)
                }
                Rectangle {
                    anchors.left: tfL.right; anchors.right: tfR.left
                    anchors.leftMargin: 2; anchors.rightMargin: 2
                    anchors.verticalCenter: parent.verticalCenter
                    anchors.verticalCenterOffset: 1
                    height: 1; color: gadget.withA(notes.paletteAccent, 0.55)
                }
            }

            // ── box-drawing bottom frame: └─ tally ─── 𝄂 ┘ ───────────────────
            Item {
                id: footer
                anchors.bottom: parent.bottom
                width: parent.width; height: 20

                Text {
                    id: ffL
                    anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter
                    text: "└─┤ " + gadget.sessions.length + " session" + (gadget.sessions.length === 1 ? "" : "s")
                          + " · " + gadget.projectCount + " project" + (gadget.projectCount === 1 ? "" : "s") + " ├"
                    font.family: gadget.faceMono; font.pixelSize: 11
                    color: gadget.withA(notes.paletteFg, 0.8)
                }
                Text {
                    id: ffCorner
                    anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                    text: "┘"
                    font.family: gadget.faceMono; font.pixelSize: 11
                    color: gadget.withA(notes.paletteAccent, 0.95)
                }
                Text {                                // final barline closes the score
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
                    height: 1; color: gadget.withA(notes.paletteAccent, 0.55)
                }
            }

            // ── THE STAVE: the roster, framed like a score sheet ──────────────
            Item {
                id: stave
                anchors.top: topFrame.bottom
                anchors.bottom: footer.top
                width: parent.width
                clip: true

                // empty stage — ASCII temple + a shrug
                Column {
                    anchors.centerIn: parent
                    spacing: 4
                    visible: gadget.sessions.length === 0
                    Text {
                        anchors.horizontalCenter: parent.horizontalCenter
                        horizontalAlignment: Text.AlignHCenter
                        font.family: gadget.faceMono; font.pixelSize: 13
                        lineHeight: 1.05
                        color: gadget.withA(notes.paletteFg, 0.45)
                        text: "    ╱▔▔▔▔▔▔▔╲\n"
                            + "   ╱ ╥ ╥ ╥ ╥ ╲\n"
                            + "  ╞═══════════╡\n"
                            + "    ║ ║ ║ ║ ║\n"
                            + "  ══╩═╩═╩═╩═╩══"
                    }
                    Item { width: 1; height: 6 }
                    Text {
                        anchors.horizontalCenter: parent.horizontalCenter
                        text: "ᕕ( ᐛ )ᕗ"
                        font.pixelSize: 20
                        color: gadget.withA(notes.paletteFg, 0.6)
                    }
                    Text {
                        anchors.horizontalCenter: parent.horizontalCenter
                        text: "TACET"
                        font.family: gadget.faceSerif; font.pixelSize: 14
                        font.letterSpacing: 5
                        color: gadget.withA(notes.paletteFg, 0.65)
                    }
                    Text {
                        anchors.horizontalCenter: parent.horizontalCenter
                        text: "no sessions on the stage"
                        font.family: gadget.faceMono; font.pixelSize: 10
                        color: gadget.withA(notes.paletteFg, 0.4)
                    }
                }

                ListView {
                    id: roster
                    anchors.fill: parent
                    visible: gadget.sessions.length > 0
                    model: gadget.sessions
                    spacing: 0
                    boundsBehavior: Flickable.StopAtBounds
                    clip: true

                    delegate: Item {
                        id: row
                        width: roster.width
                        height: 52

                        property bool emph: modelData.sessionId === gadget.emphId
                        property bool awaiting: gadget.isAwaiting(modelData.state)
                        property color accent: emph ? notes.paletteHot
                                                    : gadget.stateColor(modelData.state)

                        onAwaitingChanged: if (!awaiting) noteGlyph.opacity = 1

                        Rectangle {                    // staff ledger line
                            anchors.bottom: parent.bottom
                            width: parent.width; height: 1
                            color: gadget.withA(notes.wireCyan, 0.30)
                        }
                        Rectangle {                    // emphasis / hover wash
                            anchors.fill: parent; anchors.bottomMargin: 1
                            color: row.emph ? gadget.withA(notes.paletteHot, 0.10)
                                            : (hover.containsMouse ? gadget.withA(notes.paletteAccent, 0.08)
                                                                   : "transparent")
                        }
                        Rectangle {                    // laurel-green spine (the one standout)
                            anchors.left: parent.left; anchors.top: parent.top
                            anchors.bottom: parent.bottom; anchors.bottomMargin: 1
                            width: 3; color: notes.paletteHot; visible: row.emph
                        }

                        // the note, hung on a monospace column (pilaster + barline)
                        Item {
                            id: gutter
                            anchors.left: parent.left; anchors.leftMargin: 12
                            anchors.verticalCenter: parent.verticalCenter
                            width: 26; height: parent.height
                            Text {
                                anchors.centerIn: parent
                                text: "│"
                                font.family: gadget.faceMono; font.pixelSize: 40
                                color: row.emph ? gadget.withA(notes.paletteHot, 0.9)
                                                : gadget.withA(notes.wireCyan, 0.5)
                            }
                            Text {
                                id: noteGlyph
                                anchors.centerIn: parent
                                // Noto Music seats the notehead low in a tall em
                                // box; lift it to sit between the two text lines.
                                anchors.verticalCenterOffset: -4
                                text: gadget.glyphFor(modelData.state)
                                font.family: gadget.faceMusic; font.pixelSize: 25
                                color: row.accent
                                SequentialAnimation on opacity {
                                    running: row.awaiting
                                    loops: Animation.Infinite; alwaysRunToEnd: true
                                    NumberAnimation { to: 0.28; duration: 620; easing.type: Easing.InOutSine }
                                    NumberAnimation { to: 1.0;  duration: 620; easing.type: Easing.InOutSine }
                                }
                            }
                        }

                        Column {
                            anchors.left: gutter.right; anchors.leftMargin: 10
                            anchors.right: elapsedText.left; anchors.rightMargin: 8
                            anchors.verticalCenter: parent.verticalCenter
                            spacing: 3

                            Row {
                                spacing: 8
                                Text {
                                    id: agentName
                                    text: (modelData.agent || "session")
                                    font.family: gadget.faceSerif; font.pixelSize: 15
                                    font.weight: row.emph ? Font.Bold : Font.Medium
                                    color: notes.paletteFg
                                }
                                Text {
                                    anchors.baseline: agentName.baseline
                                    text: gadget.stateLabel(modelData.state)
                                    font.family: gadget.faceSerif; font.italic: true
                                    font.pixelSize: 11
                                    color: row.accent
                                }
                            }
                            Item {
                                width: parent.width; height: cwdText.implicitHeight
                                Text {
                                    id: cwdText
                                    anchors.left: parent.left
                                    anchors.right: kao.left; anchors.rightMargin: 6
                                    elide: Text.ElideMiddle
                                    text: gadget.shortCwd(modelData.cwd)
                                    font.family: gadget.faceMono; font.pixelSize: 10
                                    color: gadget.withA(notes.holoBlue, 0.95)
                                }
                                Text {
                                    id: kao                        // the mood face
                                    anchors.right: parent.right
                                    anchors.verticalCenter: cwdText.verticalCenter
                                    text: gadget.kaomojiFor(modelData.state)
                                    font.pixelSize: 10
                                    color: gadget.withA(row.accent, 0.85)
                                }
                            }
                        }

                        Text {                          // elapsed, tallied in gold
                            id: elapsedText
                            anchors.right: parent.right; anchors.rightMargin: 12
                            anchors.verticalCenter: parent.verticalCenter
                            text: gadget.elapsed(modelData.startedAt)
                            font.family: gadget.faceMono; font.pixelSize: 12
                            color: row.emph ? notes.paletteHot : notes.paletteAccent
                        }

                        MouseArea {
                            id: hover
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            onClicked: {
                                if (gadget.bridge && gadget.bridge.focusSession)
                                    gadget.bridge.focusSession(modelData.sessionId);
                            }
                        }
                    }
                }
            }
        }
    }
}
