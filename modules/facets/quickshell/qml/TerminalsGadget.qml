import QtQuick
import Quickshell
import Quickshell.Hyprland
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
//   · MUSIC state-glyph contract  ♪ working · 𝄐 awaiting · 𝄼 stopped · 𝄽 idle ·
//     𝄂 done · · unknown, closed by a final barline 𝄂. (glyphs = HARD CONTRACT)
//   · KAOMOJI every working row draws from MoodFaces.qml's general `working`
//     pool (terminals have no subagent concept, so the `packages` courier
//     pool never appears here), hashed from the row's stable key so its set
//     stays pinned across a delegate rebuild; ASCII / box-drawing throughout.
//   · the FUNCTION: agent · state · cwd · elapsed, click → focusSession,
//     an empty state, and hot-reload of the stage file.
//
// DIFFERENT from the Conductor (a separate temple):
//   · ORDER   — Ionic, not Doric: a volute (scroll) capital + dentil course
//               instead of the entablature band + baton course.
//   · RULE    — egg-and-dart, not the solid baton course.
//   · SIGNATURE HUE — aegean holoBlue carries the architecture (the Conductor's
//               gold recedes to a small family nod on the tag).
//   · COLUMNS — slimmer fluted twin-groove ║ pilasters, not the solid │.
//   · FRAME   — scroll-cornered box (╭ ╮ ╰ ╯), echoing the volutes.
//   · HEADER  — the inscription is centred beneath the capital, temple-front.
//   · ROWS    — TWO row silhouettes, not one card design: a bare tty stays a
//               slim ledger line (command · state · vitals · ground); an AGENT
//               terminal (claude/kimi/pi…) wears a serif nameplate + model
//               byline and a fixed SCREEN-PANE inset — a prompt line carrying
//               the live tool plus a three-line transcript preview. Both
//               silhouettes are constant-height per kind (reserved lanes), so
//               streaming data never reflows the roster. The ONE exception is
//               the ground line's cwd: it WRAPS onto new lines when the path
//               overflows (a yazi session's live dir is long and space-free —
//               a single elided line would hide which dir it is on), so a
//               long breadcrumb grows its row past the floor.
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

    // ── Special-workspace resolution ─────────────────────────────────────────
    // Hyprland names special workspaces ("magic", "scratch") and gives them
    // NEGATIVE ids — the stage only carries the numeric id, so a magic
    // terminal would read as a bare negative number (or no tag at all).
    // Resolve the name from the live workspace list; "" for regular ids and
    // for specials the list hasn't surfaced yet (tag then stays hidden, the
    // same graceful silence as a not-yet-tracked workspace).
    function workspaceName(id) {
        if (id === undefined || id === null) return ""
        var vs = (Hyprland.workspaces && Hyprland.workspaces.values) ? Hyprland.workspaces.values : []
        for (var i = 0; i < vs.length; i++)
            if (vs[i] && vs[i].id === id) return ("" + (vs[i].name || "")).toLowerCase()
        return ""
    }

    // ── CANONICAL STATE (from the rewritten daemon) ─────────────────────────────
    // Switch on the exact canonical string working|awaiting|idle|done — no regex.
    // normState() only folds a pre-rewrite stage onto the canonical four (equality,
    // not regex) so a stale file never blanks; a live daemon hits the four directly.
    function normState(state) {
        var s = (state || "").toLowerCase();
        switch (s) {
        case "working": case "awaiting": case "stopped": case "idle": case "done": return s;
        case "running": case "active": case "run": case "tool": case "busy": case "trace": return "working";
        case "blocked": case "block": case "waiting": case "wait": case "notify":
        case "notif": case "input": case "needs_input": case "needsinput":            return "awaiting";
        case "ready": case "sleep": case "sleeping":                                   return "idle";
        case "stop":                                                                   return "stopped";
        case "exit": case "exited":
        case "finished": case "finish": case "complete": case "completed":             return "done";
        default: return "";
        }
    }
    // state → notation glyph (VERBATIM contract, from conductor/theme.rs) ────────────
    function glyphFor(state) {
        switch (normState(state)) {
        case "working":  return "♪";
        case "awaiting": return "𝄐";
        case "stopped":  return "𝄼";   // a whole rest — off the desk, but recently
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
        case "stopped":  return notes.holoBlue;
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
    // ── MOOD FACES ───────────────────────────────────────────────────────────────
    // The kaomoji vocabulary — ten animated WORKING sets and one still pose per
    // resting state — lives in MoodFaces.qml, shared verbatim with the Conductor.
    property MoodFaces faces: MoodFaces {}
    // the STILL face for a state — normalized here, since this file owns the
    // state vocabulary and MoodFaces deliberately does not.
    function kaomojiFor(state) { return faces.still(normState(state)); }

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
        // The FULL breadcrumb, home-collapsed only — no last-N truncation:
        // the ground line WRAPS on overflow, so the head (which dir yazi is
        // on) stays readable; "…/tail" would hide exactly the part that
        // answers that.
        return p;
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
                say:           rec.say || "",         // the agent's latest words
                model:          rec.model || "",         // the running Claude model, if known
                contextTokens:  rec.contextTokens || 0,  // context-window fill of the last request
                contextCeiling: rec.contextCeiling || 0  // the published ceiling for that model (CONTRACTS.md §4)
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
                        r.activity, r.say, r.model, r.contextTokens,
                        r.contextCeiling].join(""));
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
        // OPAQUE + DEFINED body, matching the pantheon contract this file's own
        // header documents (hard border, inset keyline, cast shadow; no pale
        // washout). Was translucent glass (withA(paletteBg, 0.72)) letting
        // whatever sits behind the dock bleed through — every sibling gadget
        // (ConductorGadget, UsageGadget, MetersGadget, PowerVitalsGadget) fills
        // solid; Terminals was the one outlier.
        color: notes.paletteBg
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

                Rectangle {                           // deeper marble band
                    anchors.fill: parent
                    color: gadget.withA(notes.paletteFg, 0.05)
                }
                Row {
                    // khoa, live, pointed at it directly: this was the only
                    // header in the family centring its clef+name group —
                    // every sibling left-anchors instead (MetersGadget.qml's
                    // clef: plain anchors.left: parent.left). centerIn did
                    // both axes; splitting it out keeps the vertical part.
                    anchors.left: parent.left
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 8
                    Text {
                        id: clef
                        anchors.verticalCenter: parent.verticalCenter
                        anchors.verticalCenterOffset: 1   // was -3, hand-guessed and wrong; still rode
                                                           // high post horizontal-fix, nudged from a look
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

                Flickable {
                    id: flick
                    anchors.fill: parent
                    visible: gadget.rows.length > 0
                    clip: true
                    contentWidth: width
                    contentHeight: playbill.height
                    boundsBehavior: Flickable.StopAtBounds
                    flickDeceleration: 3500

                    Column {
                        id: playbill
                        width: flick.width - 6    // slim gutter for the scrollbar
                        spacing: 3

                        Repeater {
                            model: gadget.rows

                            delegate: Item {
                                id: row
                                required property var modelData
                                width: parent.width
                        // TWO constant silhouettes (see the ROWS note in the
                        // header): a bare tty settles at the 52 floor; an agent
                        // row adds the fixed screen pane — taller, but just as
                        // constant. Live data streaming in never moves either.
                        // The one exception: a cwd long enough to wrap (yazi's
                        // live dir) grows its row past the floor — the dir must
                        // stay readable, that is the point of the ground line.
                        height: Math.max(52, body.implicitHeight + 16)

                        // an emph row must have a real sessionId — plain untracked
                        // terminals ("" id) never claim the traced crown.
                        property bool emph: (modelData.sessionId || "") !== ""
                                            && modelData.sessionId === gadget.emphId
                        property bool awaiting: gadget.isAwaiting(modelData.state)
                        property bool working:  gadget.isWorking(modelData.state)
                        property color accent: emph ? notes.paletteHot
                                                    : gadget.stateColor(modelData.state)
                        // the roster's ONE structural fork: an AGENT terminal
                        // (claude/kimi/pi… — recKind falls back to agent!="shell")
                        // vs a bare tty. Agents wear the serif nameplate + model
                        // byline + screen pane + context meter; bare ttys stay
                        // slim ledger lines.
                        readonly property bool agentRow: gadget.isAgentRec(modelData)
                        readonly property string activityText: modelData.activity || ""
                        // a BARE tty's main label: the foreground command / file
                        // being edited (`nvim notes.md`, `cargo test`), or the
                        // shell process itself when idle (`bash`). An agent row's
                        // main label is its NAME instead — its activity moves
                        // into the screen pane's prompt line.
                        readonly property string procText: activityText.length > 0
                                                           ? activityText
                                                           : (modelData.agent || "shell")
                        // the agent's latest WORDS (transcript tail), flattened —
                        // the tail may carry newlines, and the pane's three-line
                        // budget is spent on wrapped prose, not blank runs.
                        // Plain ttys stay silent (the daemon never says for them).
                        readonly property string sayFlat: ("" + (modelData.say || "")).replace(/\s+/g, " ")
                        // the live Hyprland workspace this row sits on — a plain
                        // arabic number tag (NOT a note-glyph); -1 = none yet.
                        // Specials carry NEGATIVE ids; the name is resolved from
                        // the live workspace list so magic/scratch rows wear the
                        // bar's ledger marks (magic → 𝅘𝅥𝅱, scratch → 𝄋) instead
                        // of a bare negative number.
                        readonly property int wsId: (modelData.workspace !== undefined
                                                     && modelData.workspace !== null)
                                                        ? modelData.workspace : -1
                        readonly property string wsName: gadget.workspaceName(row.wsId)
                        readonly property bool isMagic:   row.wsName.indexOf("magic") !== -1
                        readonly property bool isScratch: row.wsName.indexOf("scratch") !== -1
                        readonly property bool wsSpecial: row.isMagic || row.isScratch
                        // the same hue the bar's ledger note wears for this
                        // workspace (noteColor is safe for id <= 0) — the tag
                        // and the bar's note always read in the same colour.
                        readonly property color wsTagColor: gadget.notes.noteColor(row.wsId)
                        // blocked on a `sudo` password prompt — distinct from
                        // ordinary `awaiting` ("an agent permission answer"):
                        // this reads as "it's YOUR terminal password".
                        readonly property bool needsSudo: modelData.needsSudo === true

                        onAwaitingChanged: if (!awaiting) noteGlyph.opacity = 1

                        // Re-assert the bar highlight if this row is rebuilt while
                        // it is the hovered one (roster refresh under a still pointer).
                        Component.onCompleted: {
                            if (gadget.shared && gadget.shared.hoveredSessionId !== ""
                                && gadget.shared.hoveredSessionId === gadget.rowKey(modelData))
                                gadget.setHover(gadget.rowKey(modelData),
                                                modelData.workspace !== undefined ? modelData.workspace : -1);
                        }

                        // the row plaque — ground + hairline, the Conductor's
                        // card idiom carried over at the same weights (fill
                        // 0.035, hover 0.10, laurel 0.05, awaiting border 0.75)
                        // but keyed AEGEAN: the hairline is sig-based, one step
                        // under the screen pane's own 0.30 bezel so the pane —
                        // the widget's main focus — still sits a breath forward.
                        Rectangle {
                            anchors.fill: parent
                            radius: 0
                            color: hover.containsMouse
                                   ? gadget.withA(gadget.sig, 0.10)
                                   : (row.emph ? gadget.withA(notes.paletteHot, 0.05)
                                               : gadget.withA(notes.paletteFg, 0.035))
                            border.width: 1
                            border.color: (row.awaiting || row.needsSudo)
                                          ? gadget.withA(notes.paletteUrgent, 0.75)
                                          : gadget.withA(gadget.sig, 0.28)
                        }
                        Rectangle {                    // laurel-green crown (the one standout)
                            anchors.left: parent.left; anchors.top: parent.top
                            anchors.bottom: parent.bottom
                            width: 3; color: notes.paletteHot; visible: row.emph
                        }

                        // the note, set in a carved niche on the fluted twin-
                        // groove column. The grooves are DRAWN (two 2px bars),
                        // not the 38px ║ glyph: a glyph neither spans a tall
                        // agent row nor clears the note riding on it — drawn
                        // flutes run the row's full height on every silhouette
                        // and break around a reserved niche, so column and note
                        // never collide (the lamp-box discipline, column-shaped).
                        Item {
                            id: gutter
                            anchors.left: parent.left; anchors.leftMargin: 12
                            anchors.top: parent.top; anchors.topMargin: 1
                            anchors.bottom: parent.bottom; anchors.bottomMargin: 1
                            width: 26

                            // niche centred on the note's optical seat (the -11
                            // lift below), half-height 13 → a 26px opening.
                            readonly property real nicheTop: height / 2 - 24
                            readonly property real nicheBot: height / 2 + 2
                            // an agent terminal's flute stands a shade
                            // brighter — the scan-cue for which columns
                            // hold agents (the traced crown still wins).
                            readonly property color flute:
                                row.emph ? gadget.withA(notes.paletteHot, 0.9)
                                         : gadget.withA(gadget.sig, row.agentRow ? 0.75 : 0.5)

                            Rectangle { x: 9;  y: 0; width: 2; color: gutter.flute
                                        height: Math.max(0, gutter.nicheTop) }
                            Rectangle { x: 14; y: 0; width: 2; color: gutter.flute
                                        height: Math.max(0, gutter.nicheTop) }
                            Rectangle { x: 9;  y: gutter.nicheBot; width: 2; color: gutter.flute
                                        height: Math.max(0, gutter.height - gutter.nicheBot) }
                            Rectangle { x: 14; y: gutter.nicheBot; width: 2; color: gutter.flute
                                        height: Math.max(0, gutter.height - gutter.nicheBot) }

                            Text {
                                id: noteGlyph
                                anchors.centerIn: parent
                                // Noto Music seats the notehead low in a tall em
                                // box; lift it to sit between the two text lines.
                                anchors.verticalCenterOffset: -11
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
                            anchors.right: parent.right; anchors.rightMargin: 12
                            anchors.verticalCenter: parent.verticalCenter
                            spacing: 2

                            Item {
                                width: parent.width
                                height: procName.height
                                Text {                     // the MAIN label — two voices: an
                                                            // agent row wears its NAME carved
                                                            // in serif (claude, pi — the tool
                                                            // line lives in the screen pane);
                                                            // a bare tty keeps the mono
                                                            // foreground command.
                                    id: procName
                                    anchors.left: parent.left
                                    // the state tag + lock badge claim their space
                                    // FIRST (pinned to the row's right edge below);
                                    // the label elides into the rest — the whole
                                    // left run is the name's now, the model
                                    // byline moved down to the screen pane's
                                    // prompt line (idiom cell, not a crowded
                                    // middle field).
                                    width: Math.max(24, Math.min(implicitWidth,
                                                    body.width - stateTag.slot - sudoBadge.slot))
                                    elide: Text.ElideRight
                                    text: row.agentRow ? (modelData.agent || "agent") : row.procText
                                    font.family: row.agentRow ? gadget.faceSerif : gadget.faceMono
                                    font.pixelSize: 14
                                    font.weight: row.emph ? Font.Bold : Font.Medium
                                    color: notes.paletteFg
                                }
                                Text {
                                    id: stateTag
                                    readonly property real slot: visible ? implicitWidth + 8 : 0
                                    anchors.right: parent.right
                                    anchors.baseline: procName.baseline
                                    text: gadget.stateLabel(modelData.state)
                                    font.family: gadget.faceSerif; font.italic: true
                                    font.pixelSize: 11
                                    color: row.accent
                                }
                                Text {                     // SUDO lock badge — distinct from the
                                                            // ordinary awaiting state tag: this
                                                            // terminal needs the user's password
                                                            // typed into IT, not an agent decision.
                                                            // A MONOCHROME nerd-font lock (not the
                                                            // 🔒 emoji glyph — Qt ignores Text.color
                                                            // on color-emoji glyphs, so it never
                                                            // followed the palette) rendered in the
                                                            // same mono/nerd face as the rest of the
                                                            // terminal furniture, so it DOES honor
                                                            // notes.paletteUrgent.
                                    id: sudoBadge
                                    readonly property real slot: visible ? implicitWidth + 6 : 0
                                    anchors.right: stateTag.left
                                    anchors.rightMargin: visible ? 6 : 0
                                    anchors.baseline: procName.baseline
                                    visible: row.needsSudo
                                    text: ""                     // nf-fa-lock (Nerd Font)
                                    font.family: gadget.faceMono
                                    font.pixelSize: 13
                                    color: notes.paletteUrgent
                                    // A quicker ping than the state glyph's own
                                    // awaiting pulse (620ms below) — the badge
                                    // reads as urgent on its own.
                                    SequentialAnimation on opacity {
                                        running: row.needsSudo
                                        loops: Animation.Infinite; alwaysRunToEnd: true
                                        onRunningChanged: if (!running) sudoBadge.opacity = 1.0
                                        NumberAnimation { to: 0.35; duration: 380; easing.type: Easing.InOutSine }
                                        NumberAnimation { to: 1.0;  duration: 380; easing.type: Easing.InOutSine }
                                    }
                                }
                            }
                            // ── the SCREEN PANE — agent rows only ─────────────
                            // The genuine preview: a fixed 60px inset framed as
                            // the agent's little terminal screen — a hairline
                            // aegean bezel over a faint wash (radius 0). Line one
                            // is the PROMPT: a `$` sigil + the live tool/command
                            // on the tty (the agent process itself when nothing
                            // is running). Under it, THREE reserved lines of the
                            // agent's latest words, serif-italic quoted — the
                            // transcript tail, wrapped, last line eliding. Both
                            // lanes are FIXED: streaming text fills them, the
                            // silhouette never moves. A silent agent holds the
                            // pane with dim placeholders (the stable-silhouette
                            // law); bare ttys skip the pane entirely (a Column
                            // skips invisible children) and stay slim.
                            Rectangle {
                                id: screenPane
                                visible: row.agentRow
                                width: parent.width
                                height: 60
                                radius: 0
                                color: gadget.withA(gadget.sig, 0.06)
                                border.width: 1
                                border.color: row.emph ? gadget.withA(notes.paletteHot, 0.45)
                                                       : gadget.withA(gadget.sig, 0.30)

                                Text {                     // the prompt sigil
                                    id: promptSigil
                                    x: 6; y: 4
                                    text: "$"
                                    font.family: gadget.faceMono; font.pixelSize: 10
                                    color: gadget.withA(gadget.sig, 0.95)
                                }
                                Text {                     // the live tool / command —
                                                            // flexible left field; narrows
                                                            // to make room for modelCell
                                                            // when the model is shown,
                                                            // widens to fill when it's not.
                                    anchors.left: promptSigil.right; anchors.leftMargin: 5
                                    anchors.right: modelCell.visible ? modelCell.left : parent.right
                                    anchors.rightMargin: 6
                                    anchors.baseline: promptSigil.baseline
                                    elide: Text.ElideRight
                                    text: row.activityText.length > 0
                                          ? row.activityText
                                          : (modelData.agent || "agent")
                                    font.family: gadget.faceMono; font.pixelSize: 10
                                    color: gadget.withA(notes.paletteFg,
                                                        row.activityText.length > 0 ? 0.85 : 0.45)
                                }
                                Text {                     // the MODEL — the agent's provenance,
                                                            // relocated off the crowded identity
                                                            // line into this idiom slot: a fixed
                                                            // right cell sharing the prompt line,
                                                            // baseline-matched to the $ sigil (not
                                                            // centered against the whole pane). A
                                                            // middle-elided view of the REAL model
                                                            // id (never an invented short — pantheon
                                                            // §4). Always co-occurs with the screen
                                                            // pane itself (both agent-row-only), so
                                                            // this can never orphan the display.
                                    id: modelCell
                                    visible: row.agentRow && (modelData.model || "").length > 0
                                    anchors.right: parent.right; anchors.rightMargin: 6
                                    anchors.baseline: promptSigil.baseline
                                    width: 100
                                    horizontalAlignment: Text.AlignRight
                                    elide: Text.ElideMiddle
                                    text: modelData.model || ""
                                    font.family: gadget.faceMono; font.pixelSize: 9
                                    color: gadget.withA(notes.paletteFg, 0.5)
                                }
                                Text {                     // the WORDS — 3 reserved lines
                                    anchors.left: parent.left; anchors.leftMargin: 6
                                    anchors.right: parent.right; anchors.rightMargin: 6
                                    y: 20
                                    height: 36
                                    text: row.sayFlat.length > 0 ? "“" + row.sayFlat + "”" : "…"
                                    wrapMode: Text.WordWrap
                                    maximumLineCount: 3
                                    elide: Text.ElideRight
                                    lineHeight: 12
                                    lineHeightMode: Text.FixedHeight
                                    font.family: gadget.faceSerif; font.italic: true
                                    font.pixelSize: 10
                                    color: gadget.withA(notes.paletteFg,
                                                        row.sayFlat.length > 0 ? 0.62 : 0.30)
                                }
                            }
                            // TALLY + MOOD — elapsed since the terminal opened and
                            // (agent rows) the context-window meter, sharing one
                            // line with the animated mood face: the meta tags pack
                            // the left (a Row so an invisible tag never leaves a
                            // gap), the kaomoji stays pinned to the row's right
                            // edge. The model byline rode up to the identity line
                            // and the workspace tag down to the ground line, so
                            // the meter never fights the face for room.
                            Item {
                                width: parent.width
                                height: Math.max(metaRow.implicitHeight, kao.implicitHeight)
                                Row {
                                    id: metaRow
                                    anchors.left: parent.left
                                    anchors.right: kao.left; anchors.rightMargin: 6
                                    anchors.verticalCenter: parent.verticalCenter
                                    spacing: 8
                                    Text {                     // elapsed, tallied in aegean
                                        id: elapsedText
                                        text: gadget.elapsed(modelData.startedAt)
                                        font.family: gadget.faceMono; font.pixelSize: 11
                                        color: row.emph ? notes.paletteHot : gadget.sig
                                    }
                                    Text {                     // CONTEXT-WINDOW METER — bar + percent +
                                                                // compact count, in the dock's existing
                                                                // `[▓░]` ASCII-gauge grammar (AoideBar
                                                                // battBar / MetersGadget barFill). Zero
                                                                // footprint until an assistant turn has
                                                                // produced a usage block: this Row skips
                                                                // invisible children entirely.
                                        id: ctxTag
                                        anchors.baseline: elapsedText.baseline
                                        visible: (modelData.contextTokens || 0) > 0
                                        readonly property real pct: notes.ctxPercent(modelData.contextTokens, modelData.contextCeiling)
                                        text: notes.ctxBar(pct, 6) + " " + Math.round(pct) + "% · " + notes.ctxCompact(modelData.contextTokens)
                                        font.family: gadget.faceMono; font.pixelSize: 10
                                        // song accent → paletteUrgent past ~85%, same threshold/
                                        // swap as the sudo badge's urgency grammar.
                                        color: notes.ctxColor(pct, gadget.sig)
                                    }
                                }
                                Text {
                                    id: kao                        // the mood face
                                    anchors.right: parent.right
                                    anchors.verticalCenter: parent.verticalCenter
                                    // A FIXED box, right-aligned: the frames of a
                                    // set differ in width on purpose (that width
                                    // change is the movement), and an auto-sized
                                    // Text would drag metaRow's anchor around with
                                    // it every frame.
                                    width: 96
                                    clip: true                 // a wide animation
                                                                // frame can't paint
                                                                // outside its box
                                    horizontalAlignment: Text.AlignRight
                                    // WORKING animates; every resting state holds
                                    // one pose. Terminals have no subagent concept
                                    // (that's a Conductor-only idea), so every row
                                    // wears the general `working` pool — but HASHED
                                    // from the row's stable key (pickFor/phaseFor),
                                    // never drawn at random: the underlying roster
                                    // model gets reassigned (and every delegate
                                    // recreated) on ordinary cwd/activity
                                    // heartbeats, far more often than the roster
                                    // actually changes, so a random pick would
                                    // visibly reshuffle mid-session. `rowKey` is
                                    // the same stable identity (sessionId, else
                                    // "win:"+windowAddress) already used for
                                    // hover-across-rebuild above.
                                    readonly property string moodKey: gadget.rowKey(modelData)
                                    property int setIdx: gadget.faces.pickFor(moodKey, gadget.faces.working)
                                    property int frame: gadget.faces.phaseFor(moodKey, frames.length)
                                    readonly property var frames: gadget.faces.workingFrames(setIdx)
                                    text: row.working ? frames[frame % frames.length]
                                                      : gadget.kaomojiFor(modelData.state)
                                    // Explicit family: the general pool now
                                    // carries the parcel glyph (the receiving
                                    // sets), and that icon lives in the Nerd
                                    // Font's private-use range — Qt's CJK/kana
                                    // fallback has no claim on it.
                                    font.family: gadget.faceMono
                                    font.pixelSize: 10
                                    color: gadget.withA(row.accent, 0.85)
                                    Timer {
                                        running: row.working
                                        repeat: true; interval: 300
                                        onTriggered: kao.frame = (kao.frame + 1) % kao.frames.length
                                    }
                                }
                            }
                            // the GROUND line — WHERE the terminal lives: the cwd
                            // (falls back to the window title for a cwd-less tty)
                            // with the Hyprland workspace tag holding the right
                            // corner. The dir WRAPS onto new lines when it
                            // overflows — a yazi session's live dir is a long,
                            // space-free path, and one middle-elided line would
                            // make it unreadable; a long breadcrumb grows the row.
                            // A magic/scratch row's tag is its INDICATOR: the
                            // ledger note-glyph + name (𝅘𝅥𝅱 magic / 𝄋 scratch) in
                            // the bar's own note hue — a terminal parked in the
                            // magic workspace says so outright instead of hiding
                            // behind a negative id.
                            Item {
                                width: parent.width
                                height: cwdText.implicitHeight
                                Text {                     // the workspace tag — plain "wsN", or
                                    id: wsTag              // the ledger mark + name for specials
                                    anchors.right: parent.right
                                    anchors.baseline: cwdText.baseline
                                    visible: row.wsId >= 0 || row.wsSpecial
                                    text: row.isMagic ? "𝅘𝅥𝅱 magic"
                                        : (row.isScratch ? "𝄋 scratch"
                                                         : ("ws" + row.wsId))
                                    font.family: gadget.faceMono; font.pixelSize: 10
                                    font.bold: row.wsSpecial   // the bar bolds its note glyphs too
                                    color: gadget.withA(row.wsTagColor, row.wsSpecial ? 0.95 : 0.85)
                                }
                                Text {
                                    id: cwdText
                                    anchors.left: parent.left
                                    anchors.right: wsTag.visible ? wsTag.left : parent.right
                                    anchors.rightMargin: wsTag.visible ? 8 : 0
                                    // paths carry no spaces, so WordWrap would never
                                    // break them — WrapAnywhere splits onto new lines
                                    // (yazi's live dir must stay readable); the elide
                                    // then only trims the FINAL line's tail. The
                                    // ground Item's height binds to implicitHeight,
                                    // so a long breadcrumb grows the row.
                                    wrapMode: Text.WrapAnywhere
                                    elide: Text.ElideRight
                                    text: gadget.shortCwd(modelData.cwd) || (modelData.title || "")
                                    font.family: gadget.faceMono; font.pixelSize: 10
                                    color: gadget.withA(gadget.sig, 0.95)
                                }
                            }
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

                // slim aegean scrollbar in the roster gutter — functional
                // scrolling either way; this is just the visible thumb.
                Item {
                    visible: flick.contentHeight > flick.height + 1
                    anchors.top: flick.top; anchors.bottom: flick.bottom
                    anchors.right: parent.right; anchors.rightMargin: 3
                    width: 3
                    Rectangle { anchors.fill: parent; color: gadget.withA(notes.paletteFg, 0.10) }
                    Rectangle {
                        width: parent.width
                        y: flick.visibleArea.yPosition * parent.height
                        height: Math.max(20, flick.visibleArea.heightRatio * parent.height)
                        color: gadget.withA(gadget.sig, 0.75)
                    }
                }
            }
        }
    }
}
