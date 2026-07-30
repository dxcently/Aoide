import QtQuick
import Quickshell
import Quickshell.Io

// ── THE TERMINALS ────────────────────────────────────────────────────────────
// A live view of the open terminal roster (the terminal-commander): every tty
// on the stage. A SIBLING of the Conductor in the same pantheon — but a
// different god's house. Where the Conductor is a Doric marble stele leaning
// gold, this is an IONIC temple leaning aegean (holoBlue): scroll-volute
// capital, a dentil cornice, an egg-and-dart rule, fluted twin-groove columns,
// and scroll-cornered box framing. Slimmer, more ornate, cooler in key.
//
// SHARED across the pantheon (the family resemblance):
//   · opaque + DEFINED body — hard plum border, inset keyline, cast shadow;
//     no pale washout, fully legible. All colour from `notes` roles; radius 0.
//   · MUSIC state-glyph contract  ♪ working · 𝄐 awaiting · 𝄽 idle · 𝄂 done ·
//     · unknown, closed by a final barline 𝄂. (glyphs = HARD CONTRACT)
//   · KAOMOJI mood faces give each shell life; ASCII / box-drawing throughout.
//   · the FUNCTION: agent · state · cwd · elapsed, click → focusSession,
//     an empty state, and hot-reload of the stage file.
//
// DIFFERENT from the Conductor (a separate temple):
//   · ORDER   — Ionic, not Doric: a volute (scroll) capital + dentil course
//               instead of the entablature band + Greek-key meander.
//   · RULE    — egg-and-dart, not the Greek-key meander.
//   · SIGNATURE HUE — aegean holoBlue carries the architecture (the Conductor's
//               gold recedes to a small family nod on the tag).
//   · COLUMNS — slimmer fluted twin-groove ║ pilasters, not the solid │.
//   · FRAME   — scroll-cornered box (╭ ╮ ╰ ╯), echoing the volutes.
//   · HEADER  — the inscription is centred beneath the capital, temple-front.
// The one laurel-`paletteHot` crown (the traced session) stays a pantheon-wide
// signal, identical across temples. The music state colours stay shared too —
// a "working" note reads the same in every house; only the architecture differs.
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

    // this temple's signature — aegean, not the Conductor's gold ─────────────────
    readonly property color sig: notes.holoBlue

    // live state ────────────────────────────────────────────────────────────────
    // This roster is the tty roster: EVERY live terminal window, whether or not it
    // is a tracked Aoide session. It is a PURE VIEW of ONE source — the bridge's
    // `sessions.json` (Widget–Bridge Contract). The daemon now publishes a
    // synthetic `shell` record (keyed `win:<address>`) for every untracked bare
    // terminal, so `sessions.json` alone is a complete terminal roster: the widget
    // never enumerates Hyprland itself. We just filter to the windowed records and
    // de-dupe by window address → one row per real terminal.
    property var sessions: []              // the complete stage roster (sessions.json)
    property var rows: []                   // the windowed roster actually drawn
    property real nowMs: Date.now()
    property string emphId: ""
    property int projectCount: 0
    // Signature of the RENDERED roster — rebuild() only swaps the ListView model
    // when this changes, so the frequent stage rewrites don't tear down + recreate
    // the row delegates (which would drop the hovered row's containsMouse and
    // flicker the bar highlight mid-hover).
    property string _rosterSig: ""
    property string _pendingClearId: ""    // deferred hover-clear guard

    function withA(cstr, a) {                      // alpha on a role string
        var c = Qt.darker(cstr, 1.0);
        return Qt.rgba(c.r, c.g, c.b, a);
    }

    // ── CANONICAL STATE (from the rewritten daemon) ─────────────────────────────
    // Switch on the exact canonical string working|awaiting|idle|done — no regex.
    // normState() only folds a pre-rewrite stage onto the canonical four (equality,
    // not regex) so a stale file never blanks; a live daemon hits the four directly.
    function normState(state) {
        var s = (state || "").toLowerCase();
        switch (s) {
        case "working": case "awaiting": case "idle": case "done": return s;
        case "running": case "active": case "run": case "tool": case "busy": case "trace": return "working";
        case "blocked": case "block": case "waiting": case "wait": case "notify":
        case "notif": case "input": case "needs_input": case "needsinput":            return "awaiting";
        case "ready": case "sleep": case "sleeping":                                   return "idle";
        case "stopped": case "stop": case "exit": case "exited":
        case "finished": case "finish": case "complete": case "completed":             return "done";
        default: return "";
        }
    }
    // state → notation glyph (VERBATIM contract, from baton/theme.rs) ────────────
    function glyphFor(state) {
        switch (normState(state)) {
        case "working":  return "♪";
        case "awaiting": return "𝄐";
        case "idle":     return "𝄽";
        case "done":     return "𝄂";
        default:         return "·";
        }
    }
    function isAwaiting(state) { return normState(state) === "awaiting"; }
    function isWorking(state)  { return normState(state) === "working"; }
    function stateColor(state) {
        switch (normState(state)) {
        case "awaiting": return notes.paletteUrgent;
        case "working":  return notes.paletteAccent;
        case "idle":     return notes.violet;
        case "done":     return notes.wireCyan;
        default:         return withA(notes.paletteFg, 0.45);
        }
    }
    function stateLabel(state) {
        var s = normState(state);
        return s.length ? s : "—";
    }
    // a mood face — the emoticon life ────────────────────────────────────────────
    function kaomojiFor(state) {
        switch (normState(state)) {
        case "awaiting": return "(；・∀・)";   // anxious, waiting
        case "working":  return "♪(´ε｀ )";    // humming along
        case "idle":     return "(－ω－) zzZ";  // dozing
        case "done":     return "( ´ ▽ ` )";  // content, retired
        default:         return "( ・_・)";     // puzzled
        }
    }

    // the single emphasized terminal (traced, else first working) ────────────────
    function computeEmph(list) {
        var i;
        if (shared && shared.tracedSessionId) {
            for (i = 0; i < list.length; i++)
                if (list[i].sessionId === shared.tracedSessionId)
                    return shared.tracedSessionId;
        }
        for (i = 0; i < list.length; i++)
            if (isWorking(list[i].state))
                return list[i].sessionId;
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
    // a shell prompt — the terminal reads its own cwd (or, lacking one, its window
    // title) back. Plain untracked terminals often have no stage cwd, so the live
    // window title stands in.
    function promptFor(rec) {
        var where = rec && rec.cwd ? gadget.shortCwd(rec.cwd)
                                   : (rec && rec.title ? rec.title : "");
        return (rec && rec.agent ? rec.agent : "sh") + " " + where + " $";
    }

    // ── The windowed roster: filter + de-dupe the stage records ──────────────────
    // One row per real terminal window (de-duped by window address). A record with
    // NO window address yet (a `sub:` node, or an agent hook-registered before its
    // window resolved) is NOT a terminal — it is excluded until it has one. When a
    // window is shared by two records (a conducted shell + the claude running in
    // it), the AGENT record wins — the terminal reads as what's running in it.
    // classify by `kind` (daemon-published), falling back to agent!="shell".
    function recKind(rec) {
        var k = (rec && rec.kind ? rec.kind : "").toLowerCase();
        if (k) return k;
        var a = (rec && rec.agent ? rec.agent : "").toLowerCase();
        return (a.length > 0 && a !== "shell") ? "agent" : "shell";
    }
    function isAgentRec(rec) { return recKind(rec) !== "shell"; }
    function terminalRows() {
        // group the windowed records by address; the agent record wins a shared window.
        var byAddr = ({});
        var order = [];                        // first-seen address order (stable enough pre-sort)
        for (var i = 0; i < sessions.length; i++) {
            var s = sessions[i];
            var addr = s.windowAddress || "";
            if (!addr) continue;               // no window yet → not on the tty roster
            var cur = byAddr[addr];
            if (cur === undefined) { byAddr[addr] = s; order.push(addr); }
            else if (!isAgentRec(cur) && isAgentRec(s)) { byAddr[addr] = s; }  // agent wins
        }
        var out = [];
        for (var k = 0; k < order.length; k++) {
            var rec = byAddr[order[k]];
            var wsId = (rec.workspace !== undefined && rec.workspace !== null) ? rec.workspace : -1;
            out.push({
                sessionId:     rec.sessionId || "",
                agent:         rec.agent || "shell",
                state:         rec.state || "idle",
                cwd:           rec.cwd || "",
                startedAt:     rec.startedAt || "",
                workspace:     wsId,
                windowAddress: rec.windowAddress || "",
                title:         rec.title || "",       // session name (tracked) / window title (synthetic)
                activity:      rec.activity || "",    // the current command/tool
                say:           rec.say || ""          // the agent's latest words
            });
        }
        // stable order: by workspace, then window address (so re-reads that return
        // records in a jittery order don't reshuffle the roster).
        out.sort(function (a, b) {
            return (a.workspace - b.workspace)
                || (a.windowAddress < b.windowAddress ? -1
                    : a.windowAddress > b.windowAddress ? 1 : 0);
        });
        return out;
    }

    // a signature over ONLY the fields the delegate renders (see rebuild()).
    function rosterSig(list) {
        var parts = [];
        for (var i = 0; i < list.length; i++) {
            var r = list[i];
            parts.push([r.sessionId, r.agent, r.state, r.cwd, r.startedAt,
                        r.workspace, r.windowAddress, r.title,
                        r.activity, r.say].join(""));
        }
        return parts.join("");
    }

    function rebuild() {
        var next = terminalRows();
        emphId = computeEmph(next);
        var seen = ({}), n = 0;
        for (var i = 0; i < next.length; i++) {
            var c = next[i].cwd || next[i].title || "?";
            if (!seen[c]) { seen[c] = true; n++; }
        }
        projectCount = n;
        var sig = rosterSig(next);
        if (sig !== _rosterSig || rows.length !== next.length) {
            _rosterSig = sig;
            rows = next;
        }
    }

    // ── Hover→bar highlight, made robust to delegate churn (see shell.qml) ───────
    // A stable hover key per row: the tracked sessionId, or (for a plain untracked
    // terminal with no sessionId) its window address — so the deferred-clear guard
    // and the re-assert can still identify the row across a rebuild.
    function rowKey(rec) {
        if (rec && rec.sessionId) return rec.sessionId;
        return rec && rec.windowAddress ? "win:" + rec.windowAddress : "";
    }
    function setHover(sid, ws) {
        if (!shared) return;
        _pendingClearId = "";
        shared.hoveredSessionId = sid;
        shared.hoveredWorkspace = (ws !== undefined ? ws : -1);
    }
    function requestHoverClear(sid) {
        if (!shared || shared.hoveredSessionId !== sid) return;
        _pendingClearId = sid;
        Qt.callLater(applyHoverClear);
    }
    function applyHoverClear() {
        if (_pendingClearId && shared && shared.hoveredSessionId === _pendingClearId) {
            shared.hoveredSessionId = "";
            shared.hoveredWorkspace = -1;
        }
        _pendingClearId = "";
    }

    function parseStage() {
        try {
            var t = stage.text();
            if (!t || t.trim().length === 0) { sessions = []; rebuild(); return; }
            var o = JSON.parse(t);
            sessions = (o && o.sessions) ? o.sessions : [];
        } catch (e) {
            sessions = [];
        }
        rebuild();
    }

    // the stage file — the ONLY source; hot-reloads on change ─────────────────────
    // `sessions.json` is now the complete terminal roster (the daemon publishes a
    // synthetic `shell` record per untracked tty), so this FileView is all the
    // widget needs — no `hyprctl` enumeration, per the Widget–Bridge Contract.
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

    // cast shadow — lifts the temple off any marble wallpaper ────────────────────
    Rectangle {
        anchors.fill: naos
        anchors.leftMargin: 4; anchors.topMargin: 5
        anchors.rightMargin: -4; anchors.bottomMargin: -5
        radius: 0
        color: gadget.withA(notes.paletteFg, 0.22)
    }

    // the naos (temple body) ─────────────────────────────────────────────────────
    Rectangle {
        id: naos
        anchors.fill: parent
        radius: 0
        // translucent GLASS body — the compositor blur + hyprglass frost THROUGH
        // it (like the launcher). Alpha lives on the FILL colour, not node opacity,
        // so the hard plum border + inset keyline below stay crisp at full alpha.
        color: gadget.withA(notes.paletteBg, 0.72)
        border.color: notes.paletteFg
        border.width: 2

        Rectangle {                                   // inset aegean keyline (signature)
            anchors.fill: parent; anchors.margins: 4
            radius: 0; color: "transparent"
            border.color: gadget.sig; border.width: 1
        }

        Item {
            id: content
            anchors.fill: parent
            anchors.margins: 13

            // ── IONIC CAPITAL: two volute scrolls joined by the abacus ────────
            Canvas {
                id: capital
                anchors.top: parent.top
                width: parent.width; height: 24
                onWidthChanged: requestPaint()
                onPaint: {
                    var ctx = getContext("2d");
                    ctx.reset();
                    ctx.clearRect(0, 0, width, height);
                    ctx.strokeStyle = gadget.sig;
                    ctx.lineWidth = 1.5;
                    var cy = height * 0.52;
                    var r0 = height * 0.40;
                    var lx = r0 + 2, rx = width - r0 - 2;

                    // abacus — the flat member riding the two scrolls
                    ctx.beginPath();
                    ctx.moveTo(lx, cy - r0); ctx.lineTo(rx, cy - r0);
                    ctx.stroke();

                    // a volute: a scroll spiralling into its eye
                    function volute(cx, dir) {
                        ctx.beginPath();
                        var turns = 2.15, steps = 64;
                        for (var i = 0; i <= steps; i++) {
                            var t = i / steps;
                            var ang = dir * (t * turns * 2 * Math.PI) - Math.PI / 2;
                            var r = r0 * (1 - 0.72 * t);
                            var x = cx + r * Math.cos(ang);
                            var y = cy + r * Math.sin(ang) * 0.9;
                            if (i === 0) ctx.moveTo(x, y); else ctx.lineTo(x, y);
                        }
                        ctx.stroke();
                    }
                    volute(lx,  1);
                    volute(rx, -1);
                }
            }

            // ── the inscription, centred beneath the capital ──────────────────
            Item {
                id: head
                anchors.top: capital.bottom; anchors.topMargin: 1
                width: parent.width
                height: 26

                Row {
                    anchors.centerIn: parent
                    spacing: 8
                    Text {
                        id: clef
                        anchors.verticalCenter: parent.verticalCenter
                        anchors.verticalCenterOffset: -3
                        text: "𝄢"; font.family: gadget.faceMusic; font.pixelSize: 30
                        color: gadget.sig
                    }
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        text: "TERMINALS"
                        font.family: gadget.faceSerif; font.pixelSize: 19
                        font.weight: Font.DemiBold; font.letterSpacing: 5
                        color: notes.paletteFg
                    }
                }
                Text {                                // terminal tag — a gold family nod
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    text: "[ tty ]"
                    font.family: gadget.faceMono; font.pixelSize: 11
                    color: gadget.withA(notes.paletteAccent, 0.9)
                }
            }

            // ── DENTIL COURSE: the tooth-block cornice under the capital ──────
            Row {
                id: dentils
                anchors.top: head.bottom; anchors.topMargin: 1
                anchors.horizontalCenter: parent.horizontalCenter
                height: 8
                spacing: 4
                Repeater {
                    model: Math.max(1, Math.floor((content.width) / 9))
                    Rectangle {
                        width: 5; height: 8; radius: 0
                        color: gadget.withA(gadget.sig, 0.85)
                    }
                }
            }

            // ── egg-and-dart rule (the Ionic ornament) ────────────────────────
            Canvas {
                id: eggdart
                anchors.top: dentils.bottom; anchors.topMargin: 4
                width: parent.width; height: 13
                onWidthChanged: requestPaint()
                onPaint: {
                    var ctx = getContext("2d");
                    ctx.reset();
                    ctx.clearRect(0, 0, width, height);
                    ctx.strokeStyle = gadget.sig;
                    ctx.lineWidth = 1.25;
                    var cy = height / 2;
                    var eh = height * 0.80, ew = height * 0.62;
                    var period = height * 1.7;

                    // top and bottom astragal lines frame the course
                    ctx.beginPath();
                    ctx.moveTo(0, 1); ctx.lineTo(width, 1);
                    ctx.moveTo(0, height - 1); ctx.lineTo(width, height - 1);
                    ctx.stroke();

                    for (var x = period * 0.35; x < width; x += period) {
                        // the egg — an upright oval
                        ctx.beginPath();
                        ctx.ellipse(x - ew / 2, cy - eh / 2, ew, eh);
                        ctx.stroke();
                        // the dart — a slim arrowhead between eggs
                        var dx = x + period / 2;
                        ctx.beginPath();
                        ctx.moveTo(dx, cy - eh * 0.42);
                        ctx.lineTo(dx, cy + eh * 0.30);
                        ctx.moveTo(dx - 2.4, cy + eh * 0.02);
                        ctx.lineTo(dx, cy + eh * 0.30);
                        ctx.lineTo(dx + 2.4, cy + eh * 0.02);
                        ctx.stroke();
                    }
                }
            }

            // ── scroll-cornered top frame: ╭─┤ label ├────╮ ───────────────────
            Item {
                id: topFrame
                anchors.top: eggdart.bottom; anchors.topMargin: 8
                width: parent.width; height: 15

                Text {
                    id: tfL
                    anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter
                    text: "╭─┤ ♪ open ttys ├"
                    font.family: gadget.faceMono; font.pixelSize: 11
                    color: gadget.withA(gadget.sig, 0.95)
                }
                Text {
                    id: tfR
                    anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                    text: "╮"
                    font.family: gadget.faceMono; font.pixelSize: 11
                    color: gadget.withA(gadget.sig, 0.95)
                }
                Rectangle {
                    anchors.left: tfL.right; anchors.right: tfR.left
                    anchors.leftMargin: 2; anchors.rightMargin: 2
                    anchors.verticalCenter: parent.verticalCenter
                    anchors.verticalCenterOffset: 1
                    height: 1; color: gadget.withA(gadget.sig, 0.55)
                }
            }

            // ── scroll-cornered bottom frame: ╰─ tally ─── 𝄂 ╯ ───────────────
            Item {
                id: footer
                anchors.bottom: parent.bottom
                width: parent.width; height: 20

                Text {
                    id: ffL
                    anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter
                    text: "╰─┤ " + gadget.rows.length + " terminal" + (gadget.rows.length === 1 ? "" : "s")
                          + " · " + gadget.projectCount + " cwd" + (gadget.projectCount === 1 ? "" : "s") + " ├"
                    font.family: gadget.faceMono; font.pixelSize: 11
                    color: gadget.withA(notes.paletteFg, 0.8)
                }
                Text {
                    id: ffCorner
                    anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                    text: "╯"
                    font.family: gadget.faceMono; font.pixelSize: 11
                    color: gadget.withA(gadget.sig, 0.95)
                }
                Text {                                // final barline closes the score
                    id: ffBar
                    anchors.right: ffCorner.left; anchors.rightMargin: 4
                    anchors.verticalCenter: parent.verticalCenter
                    anchors.verticalCenterOffset: -1
                    text: "𝄂"
                    font.family: gadget.faceMusic; font.pixelSize: 18
                    color: gadget.sig
                }
                Rectangle {
                    anchors.left: ffL.right; anchors.right: ffBar.left
                    anchors.leftMargin: 2; anchors.rightMargin: 6
                    anchors.verticalCenter: parent.verticalCenter
                    anchors.verticalCenterOffset: 1
                    height: 1; color: gadget.withA(gadget.sig, 0.55)
                }
            }

            // ── THE STAVE: the roster, framed like a score sheet ──────────────
            Item {
                id: stave
                anchors.top: topFrame.bottom
                anchors.bottom: footer.top
                width: parent.width
                clip: true

                // empty stage — a fluted Ionic column + a shrug
                Column {
                    anchors.centerIn: parent
                    spacing: 4
                    visible: gadget.rows.length === 0
                    Text {
                        anchors.horizontalCenter: parent.horizontalCenter
                        horizontalAlignment: Text.AlignHCenter
                        font.family: gadget.faceMono; font.pixelSize: 13
                        lineHeight: 1.05
                        color: gadget.withA(gadget.sig, 0.5)
                        text: "  ◜◝ ◜◝\n"
                            + "  (◞◟_◞◟)\n"
                            + "   ║║║║\n"
                            + "   ║║║║\n"
                            + "  ╾──────╼"
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
                        text: "no terminals on the stage"
                        font.family: gadget.faceMono; font.pixelSize: 10
                        color: gadget.withA(notes.paletteFg, 0.4)
                    }
                }

                ListView {
                    id: roster
                    anchors.fill: parent
                    visible: gadget.rows.length > 0
                    model: gadget.rows
                    spacing: 0
                    boundsBehavior: Flickable.StopAtBounds
                    clip: true

                    delegate: Item {
                        id: row
                        width: roster.width
                        // grows to fit name · activity · prompt (52 floor).
                        height: Math.max(52, body.implicitHeight + 16)

                        // an emph row must have a real sessionId — plain untracked
                        // terminals ("" id) never claim the traced crown.
                        property bool emph: (modelData.sessionId || "") !== ""
                                            && modelData.sessionId === gadget.emphId
                        property bool awaiting: gadget.isAwaiting(modelData.state)
                        property color accent: emph ? notes.paletteHot
                                                    : gadget.stateColor(modelData.state)
                        readonly property string activityText: modelData.activity || ""
                        // the row's MAIN label: what the terminal is running — the
                        // foreground command / file being edited (`nvim notes.md`,
                        // `cargo test`), or the shell/agent process itself when idle
                        // (`bash`, `claude`). The dir rides below as subtext.
                        readonly property string procText: activityText.length > 0
                                                           ? activityText
                                                           : (modelData.agent || "shell")
                        // the agent's latest WORDS (transcript tail) — distinct
                        // from procText (the process). Plain ttys stay silent.
                        readonly property string sayText: modelData.say || ""

                        onAwaitingChanged: if (!awaiting) noteGlyph.opacity = 1

                        // Re-assert the bar highlight if this row is rebuilt while
                        // it is the hovered one (roster refresh under a still pointer).
                        Component.onCompleted: {
                            if (gadget.shared && gadget.shared.hoveredSessionId !== ""
                                && gadget.shared.hoveredSessionId === gadget.rowKey(modelData))
                                gadget.setHover(gadget.rowKey(modelData),
                                                modelData.workspace !== undefined ? modelData.workspace : -1);
                        }

                        Rectangle {                    // staff ledger line
                            anchors.bottom: parent.bottom
                            width: parent.width; height: 1
                            color: gadget.withA(gadget.sig, 0.28)
                        }
                        Rectangle {                    // emphasis / hover wash
                            anchors.fill: parent; anchors.bottomMargin: 1
                            color: row.emph ? gadget.withA(notes.paletteHot, 0.10)
                                            : (hover.containsMouse ? gadget.withA(gadget.sig, 0.09)
                                                                   : "transparent")
                        }
                        Rectangle {                    // laurel-green crown (the one standout)
                            anchors.left: parent.left; anchors.top: parent.top
                            anchors.bottom: parent.bottom; anchors.bottomMargin: 1
                            width: 3; color: notes.paletteHot; visible: row.emph
                        }

                        // the note, hung on a slim fluted twin-groove column
                        Item {
                            id: gutter
                            anchors.left: parent.left; anchors.leftMargin: 12
                            anchors.verticalCenter: parent.verticalCenter
                            width: 26; height: parent.height
                            Text {
                                anchors.centerIn: parent
                                text: "║"
                                font.family: gadget.faceMono; font.pixelSize: 38
                                color: row.emph ? gadget.withA(notes.paletteHot, 0.9)
                                                : gadget.withA(gadget.sig, 0.5)
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
                            id: body
                            anchors.left: gutter.right; anchors.leftMargin: 10
                            anchors.right: elapsedText.left; anchors.rightMargin: 8
                            anchors.verticalCenter: parent.verticalCenter
                            spacing: 2

                            Row {
                                width: parent.width
                                spacing: 8
                                Text {                     // the PROCESS / command / file being edited
                                    id: procName
                                    width: Math.min(implicitWidth, body.width - 78)
                                    elide: Text.ElideRight
                                    text: row.procText
                                    font.family: gadget.faceMono; font.pixelSize: 14
                                    font.weight: row.emph ? Font.Bold : Font.Medium
                                    color: notes.paletteFg
                                }
                                Text {
                                    anchors.baseline: procName.baseline
                                    text: gadget.stateLabel(modelData.state)
                                    font.family: gadget.faceSerif; font.italic: true
                                    font.pixelSize: 11
                                    color: row.accent
                                }
                            }
                            // SAY — a claude terminal's latest words, tail-read from
                            // its transcript by the bridge; dim quoted prose, up to
                            // two lines. Hidden for plain ttys / a silent agent.
                            Text {
                                width: parent.width
                                visible: row.sayText.length > 0
                                text: "“" + row.sayText + "”"
                                wrapMode: Text.WordWrap
                                maximumLineCount: 2
                                elide: Text.ElideRight
                                font.family: gadget.faceSerif; font.italic: true
                                font.pixelSize: 10
                                color: gadget.withA(notes.paletteFg, 0.55)
                            }
                            // the DIR — the terminal's working directory, as subtext
                            // (falls back to the window title for a cwd-less tty).
                            Item {
                                width: parent.width; height: cwdText.implicitHeight
                                Text {
                                    id: cwdText
                                    anchors.left: parent.left
                                    anchors.right: kao.left; anchors.rightMargin: 6
                                    elide: Text.ElideMiddle
                                    text: gadget.shortCwd(modelData.cwd) || (modelData.title || "")
                                    font.family: gadget.faceMono; font.pixelSize: 10
                                    color: gadget.withA(gadget.sig, 0.95)
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

                        Text {                          // elapsed, tallied in aegean
                            id: elapsedText
                            anchors.right: parent.right; anchors.rightMargin: 12
                            anchors.verticalCenter: parent.verticalCenter
                            text: gadget.elapsed(modelData.startedAt)
                            font.family: gadget.faceMono; font.pixelSize: 12
                            color: row.emph ? notes.paletteHot : gadget.sig
                        }

                        MouseArea {
                            id: hover
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            // hovering a roster row lights up that terminal's
                            // workspace on the bar (shared.hoveredWorkspace →
                            // WorkspaceRow preview highlight). Keyed on rowKey and
                            // cleared through the deferred guard so a roster refresh
                            // can't strand or wipe a hover the pointer still holds.
                            onEntered: gadget.setHover(gadget.rowKey(modelData),
                                                       modelData.workspace !== undefined ? modelData.workspace : -1)
                            onExited:  gadget.requestHoverClear(gadget.rowKey(modelData))
                            onClicked: {
                                // Every row now carries a sessionId — a tracked
                                // agent/shell, or the daemon's synthetic `win:<addr>`
                                // for a bare tty. The bridge resolves it to a window
                                // (the one narrow outbound socket verb).
                                if (modelData.sessionId && gadget.bridge && gadget.bridge.focusSession)
                                    gadget.bridge.focusSession(modelData.sessionId);
                            }
                        }
                    }
                }
            }
        }
    }
}
