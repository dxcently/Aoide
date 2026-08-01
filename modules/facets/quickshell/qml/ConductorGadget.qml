import QtQuick
import Quickshell
import Quickshell.Io

// ── THE CONDUCTOR ────────────────────────────────────────────────────────────
// A live view of the agent-session roster (the `aoide conductor`):
// a Doric temple rendered in a terminal, keeping a musical score.
//
// Four voices, layered so each earns its place:
//   · GREEK      — an opaque marble stele: plum-ink border, inset Attic-gold
//                  keyline, a treble clef + carved serif inscription, and a
//                  drawn Greek-key meander ruling off the entablature.
//   · MUSIC      — the roster is a stave; each session is a note on a ledger
//                  line; its live state is spoken as a notation glyph
//                  (♪ working · 𝄐 awaiting · 𝄼 stopped · 𝄽 idle · 𝄂 done ·
//                  · unknown), and a final barline 𝄂 closes the score.
//                  (glyphs = HARD CONTRACT). State is ALSO spoken by the
//                  kaomoji layer below — the note glyph itself no longer
//                  bobs or shimmers; that motion moved entirely onto the
//                  face. The awaiting row wash + note-pulse are kept.
//
// ROSTER = only what a conductor conducts: agent sessions + the shells an agent
// is attached to (shared `windowAddress`); bare unattended shells are hidden.
// Rows are colour-coded by agent identity (base16 noteColor spread) over the
// state spread, with the ONE laurel `paletteHot` reserved for the traced row.
//   · TERMINAL   — box-drawing frames the roster like a TUI panel; each row
//                  hangs from a monospace column │ (pilaster + staff barline).
//   · KAOMOJI    — every working row wears one of THREE pools (MoodFaces.qml),
//                  each pinned by a hash of the row's identity so the set
//                  never reshuffles under it, even though the underlying
//                  roster model gets swapped (and every delegate recreated)
//                  on ordinary activity heartbeats: a SUBAGENT wears the
//                  `packages` courier pool, hashed from its own id; a
//                  top-level agent currently coordinating an active subagent
//                  wears the `receiving` pool (catch/inbox), hashed from its
//                  id + a ":subs" suffix so the pin deliberately changes when
//                  it transitions in/out of that state; every other
//                  top-level agent wears the general `working` pool, hashed
//                  from its own id. Resting states (awaiting/stopped/idle/
//                  done) each hold one still pose. The empty stage gets an
//                  ASCII temple and a shrug.
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
    property var sessions: []              // raw roster (every stage record)
    property var visibleRows: []           // filtered roster actually drawn
    property var agentIndex: ({})           // effective-agent name → identity slot
    property real nowMs: Date.now()
    property string emphId: ""
    property int projectCount: 0
    // Signature of the RENDERED roster — recompute() only swaps the ListView
    // model when this changes, so the frequent no-op sessions.json rewrites
    // (agents heartbeat while their state/cwd stay put) don't tear down and
    // rebuild the row delegates. A live delegate keeps its MouseArea's
    // containsMouse, so a hover over the bar-highlight row survives the reload.
    property string _rosterSig: ""
    // A deferred hover-clear guard (see setHover/requestHoverClear below).
    property string _pendingClearId: ""

    function withA(cstr, a) {                      // alpha on a role string
        var c = Qt.darker(cstr, 1.0);
        return Qt.rgba(c.r, c.g, c.b, a);
    }

    // ── CANONICAL STATE (from the rewritten daemon) ─────────────────────────────
    // The daemon now emits exactly one of working|awaiting|idle|done, verbatim, so
    // we SWITCH on the exact string — no regex derivation. normState() only folds a
    // pre-rewrite stage (a stale "running"/"blocked"/…) onto the canonical four by
    // plain equality, so the roster never blanks mid-transition; a live daemon hits
    // the first four cases directly.
    function normState(state) {
        var s = (state || "").toLowerCase();
        switch (s) {
        case "working": case "awaiting": case "stopped": case "idle": case "done": return s;
        // legacy-compat aliases (equality, not regex) — remove once fully migrated
        case "running": case "active": case "run": case "tool": case "busy": case "trace": return "working";
        case "blocked": case "block": case "waiting": case "wait": case "notify":
        case "notif": case "input": case "needs_input": case "needsinput":            return "awaiting";
        case "ready": case "sleep": case "sleeping":                                   return "idle";
        case "stop":                                                                   return "stopped";
        case "exit": case "exited":
        case "finished": case "finish": case "complete": case "completed":             return "done";
        default: return "";                                                            // unknown
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
    function isIdle(state)     { return normState(state) === "idle"; }
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

    // ── MOOD FACES ───────────────────────────────────────────────────────────────
    // The kaomoji vocabulary — ten animated WORKING sets and one still pose per
    // resting state — lives in MoodFaces.qml, shared verbatim with the Terminals
    // temple. Held as a plain child object: it is a table plus pure lookups, so
    // there is nothing to thread down from shell.qml the way DrachmaState is.
    property MoodFaces faces: MoodFaces {}
    // the STILL face for a state — normalized here, since this file owns the
    // state vocabulary and MoodFaces deliberately does not. The `true` marks
    // these rows as AGENTS: an idle agent stares blankly ('_') where an idle
    // terminal sleeps.
    function kaomojiFor(state) { return faces.still(normState(state), true); }

    // ── ROSTER FILTER (concepts/Conductor-Channel) ───────────────────────────────
    // A conductor's stage shows only what it conducts:
    //   (a) AGENT sessions      — `agent` is a real agent (e.g. claude), not a
    //                             bare "shell"; and
    //   (b) agent-ATTACHED shells — a conductable shell that SHARES its Hyprland
    //                             `windowAddress` with an agent session (the agent
    //                             runs inside that very terminal window).
    // Plain shells with no agent in their window are hidden. The shared window
    // address IS the connection signal — the SessionRecord.windowAddress written
    // by shellbridge (graph.rs) and mirrored on graph.json nodes; two records that
    // agree on it live in one terminal, so the agent is conducting that shell.
    // CLASSIFY BY `kind` (published by the daemon), not by the agent name. A record
    // with kind "agent"|"subagent" is a conductable agent; "shell" is a terminal.
    // When kind is absent (legacy stage), fall back to agent!="shell".
    function recKind(rec) {
        var k = (rec && rec.kind ? rec.kind : "").toLowerCase();
        if (k) return k;
        var a = (rec && rec.agent ? rec.agent : "").toLowerCase();
        return (a.length > 0 && a !== "shell") ? "agent" : "shell";
    }
    function isShellRec(rec) { return recKind(rec) === "shell"; }
    function isAgentRec(rec) { return !isShellRec(rec); }   // agent OR subagent
    function isSubagentRec(rec) { return recKind(rec) === "subagent"; }
    // Whether `sessionId` currently has at least one ACTIVE (working) subagent
    // child — used to bias a top-level row's kaomoji toward the `receiving`
    // pool while it's coordinating dispatched work, distinct from a plain
    // solo agent. Derived from the raw roster (`sessions`), not the
    // flattened/depth-clamped `visibleRows`, so it reflects the true parent/
    // child relationship regardless of display tree depth.
    function hasActiveSubagents(sessionId) {
        if (!sessionId) return false;
        for (var i = 0; i < sessions.length; i++) {
            var r = sessions[i];
            if ((r.parentSessionId || "") === sessionId && isSubagentRec(r) && isWorking(r.state))
                return true;
        }
        return false;
    }
    function conductorFor(rec) {            // agent name sharing this shell's window, else ""
        var wa = rec && rec.windowAddress ? rec.windowAddress : "";
        if (!wa) return "";
        for (var i = 0; i < sessions.length; i++) {
            var o = sessions[i];
            if (o !== rec && isAgentRec(o) && (o.windowAddress || "") === wa)
                return o.agent;
        }
        return "";
    }
    // parentSessionId → [child records], built from the FULL roster (no ordering
    // assumption). A subagent's parent is its spawning session (or a parent sub:
    // node); a nested claude's parent is the session it launched inside.
    function childrenIndex() {
        var idx = ({});
        for (var i = 0; i < sessions.length; i++) {
            var r = sessions[i];
            var p = r.parentSessionId || "";
            if (!p) continue;
            if (!idx[p]) idx[p] = [];
            idx[p].push(r);
        }
        return idx;
    }
    function shallow(rec) { var c = {}; for (var k in rec) c[k] = rec[k]; return c; }
    // ── ROSTER as a beamed TREE ──────────────────────────────────────────────────
    // Roots are agent/subagent records that are NOT nested under another agent in
    // the roster (top-level, or orphaned by a missing parent). Each root is emitted
    // at _depth 0, then its agent/subagent descendants are walked and emitted at
    // _depth 1 — anything deeper than a grandchild is FLATTENED to that same indent
    // (Math.min(depth+1,1)). Shells never nest: a bare agent-attached shell still
    // shows as a flat depth-0 row (today's behaviour), and shell children are
    // skipped from the tree (the Terminals temple owns shells).
    function computeVisible() {
        var i, r;
        var kids = childrenIndex();
        var byId = ({});
        for (i = 0; i < sessions.length; i++)
            if (sessions[i].sessionId) byId[sessions[i].sessionId] = sessions[i];

        var out = [], visited = ({});
        function cmp(a, b) {
            var ta = Date.parse(a.startedAt || "") || 0, tb = Date.parse(b.startedAt || "") || 0;
            return (ta - tb) || ((a.sessionId || "") < (b.sessionId || "") ? -1
                                 : (a.sessionId || "") > (b.sessionId || "") ? 1 : 0);
        }
        function walk(rec, depth, parentEff) {
            if (rec.sessionId && visited[rec.sessionId]) return;
            if (rec.sessionId) visited[rec.sessionId] = true;
            var c = shallow(rec);
            c._depth = depth;
            c._parentAgent = parentEff || "";
            out.push(c);
            var cs = (kids[rec.sessionId] || []).slice().sort(cmp);
            var eff = effAgent(rec);
            for (var j = 0; j < cs.length; j++) {
                if (isShellRec(cs[j])) continue;         // shells stay out of the tree
                walk(cs[j], Math.min(depth + 1, 1), eff); // clamp: grandchildren+ → depth 1
            }
        }
        for (i = 0; i < sessions.length; i++) {
            r = sessions[i];
            // No shell emits in the Conductor: it is the AGENT tree. A conducted
            // shell that hosts a claude is represented BY that claude (which emits
            // as its own root just below); every other shell lives in the Terminals
            // temple. This drops the redundant "shell → claude" host row so one
            // terminal reads as exactly one row.
            if (isShellRec(r)) continue;
            var p = r.parentSessionId || "";
            if (p && byId[p] && !isShellRec(byId[p])) continue;  // nested → emitted by its parent
            walk(r, 0, "");
        }
        return out;
    }
    function effAgent(rec) {                // the identity a row colour-codes by
        return isAgentRec(rec) ? rec.agent : (rec && rec._conductedBy ? rec._conductedBy : "");
    }
    // stable per-agent identity hue, from the base16 accent spread (noteColor) —
    // conducted shells borrow their conductor's hue so a pair reads as one voice.
    function hueForAgent(k) {
        var idx = agentIndex[k];
        if (!k || k.length === 0 || idx === undefined) return notes.holoBlue;
        return notes.noteColor ? notes.noteColor(idx + 1) : notes.holoBlue;
    }
    function hueFor(rec) { return hueForAgent(effAgent(rec)); }

    // the single emphasized session (traced, else first working) — over VISIBLE ──
    function computeEmph(rows) {
        var i;
        if (shared && shared.tracedSessionId) {
            for (i = 0; i < rows.length; i++)
                if (rows[i].sessionId === shared.tracedSessionId)
                    return shared.tracedSessionId;
        }
        for (i = 0; i < rows.length; i++)
            if (isWorking(rows[i].state))
                return rows[i].sessionId;
        return "";
    }

    // a signature over ONLY the fields the delegate renders — so a rewrite that
    // touches nothing visible (a heartbeat timestamp, a socket path) yields the
    // same string and we keep the existing delegates (and the live hover) intact.
    function rosterSig(rows) {
        var parts = [];
        for (var i = 0; i < rows.length; i++) {
            var r = rows[i];
            parts.push([r.sessionId, r.agent, r.state, r.cwd, r.startedAt,
                        r.workspace, r._conductedBy, r.title, r.activity, r.say,
                        r.kind, r.parentSessionId, r._depth, r._parentAgent].join(""));
        }
        return parts.join("");
    }

    // ── Hover→bar highlight, made robust to delegate churn ───────────────────────
    // The row delegate calls these instead of poking `shared` directly. Hover is
    // keyed on the row's sessionId so a rebuilt delegate can re-assert it, and the
    // clear is DEFERRED (Qt.callLater) + cancellable so a teardown-triggered exit
    // that is immediately followed by the same row's re-creation does not wipe a
    // hover the pointer still holds.
    function setHover(sid, ws) {
        if (!shared) return;
        _pendingClearId = "";                       // cancel any deferred clear
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
        var next = computeVisible();
        // assign each distinct effective-agent a stable identity slot (order of
        // first appearance), so hueFor() cycles the base16 spread deterministically.
        var idx = ({}), slot = 0, i;
        for (i = 0; i < next.length; i++) {
            var k = effAgent(next[i]);
            if (k.length > 0 && idx[k] === undefined) { idx[k] = slot; slot++; }
        }
        agentIndex = idx;
        emphId = computeEmph(next);
        var seen = ({}), n = 0;
        for (i = 0; i < next.length; i++) {
            var c = next[i].cwd || "?";
            if (!seen[c]) { seen[c] = true; n++; }
        }
        projectCount = n;
        // Only reassign the ListView model when the rendered roster actually
        // changed. agentIndex/emphId/projectCount above are plain properties whose
        // re-evaluation does NOT recreate delegates; `visibleRows` is the model, so
        // holding its reference stable across no-op reloads is what stops the flicker.
        var sig = rosterSig(next);
        if (sig !== _rosterSig || visibleRows.length !== next.length) {
            _rosterSig = sig;
            visibleRows = next;
        }
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

            // ── ENTABLATURE: clef · inscription · [conductor] tag ─────────────
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
                    text: "[ conductor ]"
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
                    text: "└─┤ " + gadget.visibleRows.length + " session" + (gadget.visibleRows.length === 1 ? "" : "s")
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
                    visible: gadget.visibleRows.length === 0
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
                        text: "no agent sessions"
                        font.family: gadget.faceMono; font.pixelSize: 10
                        color: gadget.withA(notes.paletteFg, 0.4)
                    }
                }

                ListView {
                    id: roster
                    anchors.fill: parent
                    visible: gadget.visibleRows.length > 0
                    model: gadget.visibleRows
                    spacing: 0
                    boundsBehavior: Flickable.StopAtBounds
                    clip: true

                    delegate: Item {
                        id: row
                        width: roster.width
                        // grows to fit name · activity · cwd; a 52 floor keeps the
                        // stave rhythm for the flat (no-activity) rows.
                        height: Math.max(52, body.implicitHeight + 16)

                        property bool emph: modelData.sessionId === gadget.emphId
                        // the three live motion states, off the CANONICAL state
                        // (exact switch, no regex).
                        property bool working:  gadget.isWorking(modelData.state)
                        property bool awaiting: gadget.isAwaiting(modelData.state)
                        property bool idle:     gadget.isIdle(modelData.state)
                        // per-agent identity hue (base16 spread); conducted shells
                        // borrow their conductor's hue.
                        property color idHue: gadget.hueFor(modelData)
                        property string conductedBy: modelData._conductedBy || ""
                        property color accent: emph ? notes.paletteHot
                                                    : gadget.stateColor(modelData.state)

                        // ── tree/beaming ────────────────────────────────────────
                        readonly property int depth: modelData._depth || 0
                        readonly property bool isChild: depth > 0
                        readonly property int indent: isChild ? 24 : 0
                        readonly property bool subagent: gadget.isSubagentRec(modelData)
                        // the MAIN agent — a conductable, non-subagent row (the
                        // top-of-tree Claude session, as opposed to a Task it
                        // dispatched, or a plain conducted shell).
                        readonly property bool mainAgent: gadget.isAgentRec(modelData) && !subagent
                        readonly property bool hasTitle: (modelData.title || "").length > 0
                        // whether this session's OWN running Claude model is known
                        // yet (agent and subagent alike each carry their own).
                        readonly property bool modelKnown: (modelData.model || "").length > 0
                        // whether the subagent TYPE tag has something to show — a
                        // title took the name line, so the type (general-purpose,
                        // Plan, …) needs its own spot; title-less subagents already
                        // wear their type as nameText, so this stays false then.
                        readonly property bool showType: subagent && hasTitle
                            && (modelData.agent || "").length > 0
                        // the row's display name: the human session name (title),
                        // else the agent.
                        // the row NAME: the session name (title — Claude's own
                        // session title, e.g. "Aoide Dev", bridged from the
                        // transcript), else the agent. The Conductor is the AGENT
                        // view — it names by session, not by cwd (that is the
                        // Terminals temple's subtext).
                        readonly property string nameText: hasTitle ? modelData.title
                                                                    : (modelData.agent || "session")
                        readonly property string activityText: modelData.activity || ""
                        // the agent's latest WORDS (transcript tail) — distinct
                        // from activityText (the current tool). Shells never speak.
                        readonly property string sayText: modelData.say || ""
                        // a beam is coloured from the PARENT's identity hue so the
                        // group reads as one gesture (falls back to own hue).
                        readonly property color beamHue: gadget.hueForAgent(
                            (modelData._parentAgent && modelData._parentAgent.length > 0)
                                ? modelData._parentAgent : gadget.effAgent(modelData))
                        // the live Hyprland workspace this row sits on — a plain
                        // arabic number tag (NOT a note-glyph); -1 = none yet.
                        readonly property int wsId: (modelData.workspace !== undefined
                                                     && modelData.workspace !== null)
                                                        ? modelData.workspace : -1

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
                        // AWAITING — a terracotta attention pulse washes the whole
                        // row (distinct from working's travelling shimmer). The
                        // stalled state is unmistakable at row scale.
                        Rectangle {
                            id: awaitWash
                            anchors.fill: parent; anchors.bottomMargin: 1
                            color: notes.paletteUrgent
                            opacity: 0.0
                            visible: row.awaiting
                            SequentialAnimation on opacity {
                                running: row.awaiting
                                loops: Animation.Infinite; alwaysRunToEnd: true
                                onRunningChanged: if (!running) awaitWash.opacity = 0.0
                                NumberAnimation { to: 0.16; duration: 560; easing.type: Easing.OutCubic }
                                NumberAnimation { to: 0.0;  duration: 640; easing.type: Easing.InOutSine }
                            }
                        }
                        Rectangle {                    // laurel-green spine (the one standout)
                            anchors.left: parent.left; anchors.top: parent.top
                            anchors.bottom: parent.bottom; anchors.bottomMargin: 1
                            width: 3; color: notes.paletteHot; visible: row.emph
                        }

                        // ── TREE LIMB: a child hangs off its parent as a drawn
                        // elbow — a stem dropping from the parent's row above,
                        // turning into a limb that runs all the way to the child's
                        // name. The child carries NO notehead of its own: the limb
                        // plus the gutter's width is the indent, so the rows read
                        // as a tree rather than as two floating notes. Coloured
                        // from the PARENT's identity hue, so a group reads as one.
                        Item {
                            id: beam
                            visible: row.isChild
                            anchors.left: parent.left; anchors.leftMargin: 18
                            anchors.top: parent.top; anchors.topMargin: 6
                            anchors.bottom: gutter.verticalCenter
                            anchors.bottomMargin: -2
                            // runs past the (empty) gutter to just short of the body
                            width: row.indent + 24
                            Rectangle {                // the stem, rising to the parent stave
                                anchors.left: parent.left
                                anchors.top: parent.top; anchors.bottom: parent.bottom
                                width: 1.5; radius: 0
                                color: gadget.withA(row.beamHue, 0.55)
                            }
                            Rectangle {                // the beam bar (a quaver beam)
                                anchors.left: parent.left; anchors.right: parent.right
                                anchors.bottom: parent.bottom
                                height: 2; radius: 0
                                color: gadget.withA(row.beamHue, 0.85)
                            }
                        }

                        // the note, hung on a monospace column (pilaster + barline)
                        Item {
                            id: gutter
                            anchors.left: parent.left; anchors.leftMargin: 12 + row.indent
                            anchors.verticalCenter: parent.verticalCenter
                            width: 26; height: parent.height
                            Text {                     // pilaster — tinted by agent identity
                                anchors.centerIn: parent
                                visible: !row.isChild   // children hang off the beam, not a barline
                                text: "│"
                                font.family: gadget.faceMono; font.pixelSize: 40
                                color: row.emph ? gadget.withA(notes.paletteHot, 0.9)
                                                : gadget.withA(row.idHue, 0.6)
                            }
                            Text {
                                id: noteGlyph
                                // Children wear no note — the tree limb carries
                                // them, and the gutter it crosses is the indent.
                                visible: !row.isChild
                                anchors.centerIn: parent
                                // Noto Music seats the notehead low in a tall em
                                // box; lift it to sit between the two text lines.
                                anchors.verticalCenterOffset: -4
                                text: gadget.glyphFor(modelData.state)
                                font.family: gadget.faceMusic
                                font.pixelSize: row.isChild ? 20 : 25   // child note ~0.8×
                                color: row.accent
                                // The note holds STILL. Motion moved to the mood
                                // face (see `kao`), which now carries the row's
                                // whole sense of life — the note bobbing, pulsing
                                // and breathing under it read as competing tics.
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
                                height: agentName.height
                                Text {                     // the session NAME (title, else agent)
                                    id: agentName
                                    anchors.left: parent.left
                                    // The state tag claims its space FIRST (pinned
                                    // to the row's right edge below) and the name
                                    // elides into whatever is left — a fixed
                                    // reserve let a long title push the tag off the
                                    // row's right edge.
                                    width: Math.max(24, Math.min(implicitWidth,
                                                    body.width - stateTag.slot))
                                    elide: Text.ElideRight
                                    text: row.nameText
                                    font.family: gadget.faceSerif
                                    font.pixelSize: row.isChild ? 13 : 15
                                    font.weight: row.emph ? Font.Bold : Font.Medium
                                    color: notes.paletteFg
                                }
                                Text {
                                    id: stateTag
                                    readonly property real slot: visible ? implicitWidth + 8 : 0
                                    anchors.right: parent.right
                                    anchors.baseline: agentName.baseline
                                    text: gadget.stateLabel(modelData.state)
                                    font.family: gadget.faceSerif; font.italic: true
                                    font.pixelSize: 11
                                    color: row.accent
                                }
                            }
                            // TALLY — the row's metadata line, composed into two
                            // distinct grammars (a shell keeps the third, older
                            // one). Both separators are drawn from the widget's OWN
                            // glyph grammar rather than a generic UI arrow — "‖" (a
                            // musical repeat/parallel bar) reads as the subordinate,
                            // ECHOING voice a subagent is; "⟐" is the diamond that
                            // used to sit as the model tag's own prefix, repurposed
                            // here as the join it was always halfway to being, for
                            // the PRIMARY (main-agent) line — the two are shape- and
                            // meaning-distinct at a glance, not just different chars:
                            //   subagent:   elapsed  model ‖ type       ("‖" joins
                            //               a subagent's OWN model to its TYPE —
                            //               general-purpose, Plan, …)
                            //   main agent: model ⟐ ws                 ("⟐" joins
                            //               the running model to the workspace it's
                            //               conducting on — a DIFFERENT glyph than
                            //               the subagent's, so the two grammars
                            //               read apart at a glance; elapsed drops
                            //               out here once a model is known, for the
                            //               compact read the screenshot called for,
                            //               but stays as a fallback so the line is
                            //               never blank before that).
                            //   shell:      elapsed ⇢ conductor  ws    (unchanged)
                            // rides UNDER the name so the name line stays a name and
                            // a state; pinned to the row's right edge the elapsed
                            // time collided with the wrapped say prose and, on a
                            // child row, with the kaomoji.
                            Row {
                                width: parent.width
                                spacing: 8
                                Text {                     // elapsed, tallied in gold — always for
                                                            // a subagent/shell; a main agent drops it
                                                            // once its model is known (see the tally
                                                            // note above), kept meanwhile as a fallback.
                                    id: elapsedText
                                    visible: !row.mainAgent || !row.modelKnown
                                    text: gadget.elapsed(modelData.startedAt)
                                    font.family: gadget.faceMono; font.pixelSize: 11
                                    color: row.emph ? notes.paletteHot : notes.paletteAccent
                                }
                                Text {                     // the running Claude model, when known —
                                                            // each session's OWN model (a subagent's
                                                            // may differ from its parent's).
                                    id: agentTag
                                    anchors.baseline: elapsedText.baseline
                                    visible: row.modelKnown
                                    text: modelData.model
                                    font.family: gadget.faceMono; font.pixelSize: 10
                                    color: row.idHue
                                }
                                Text {                     // subagent separator — joins the model to
                                                            // the type tag; only when both are present.
                                                            // A musical parallel/repeat bar, distinct in
                                                            // shape (not just character) from the main
                                                            // agent's "⟐" below.
                                    id: subSep
                                    anchors.baseline: elapsedText.baseline
                                    visible: row.subagent && row.modelKnown && row.showType
                                    text: "‖"
                                    font.family: gadget.faceMono; font.pixelSize: 10
                                    color: row.idHue
                                }
                                Text {                     // the subagent's TYPE (general-purpose, Plan, …) —
                                                            // a title took the name line, so a titled subagent
                                                            // needs it back here, alongside its model.
                                                            // Title-less subagents already wear their type as
                                                            // nameText, so this stays hidden then (no double-up).
                                    id: subagentTypeTag
                                    anchors.baseline: elapsedText.baseline
                                    visible: row.showType
                                    text: modelData.agent
                                    font.family: gadget.faceMono; font.pixelSize: 10
                                    color: row.idHue
                                }
                                Text {                     // main-agent separator — joins the model to
                                                            // the workspace; the diamond that used to
                                                            // prefix the model tag itself, repurposed
                                                            // (deliberately a DIFFERENT glyph than the
                                                            // subagent's "‖" above).
                                    id: mainSep
                                    anchors.baseline: elapsedText.baseline
                                    visible: row.mainAgent && row.modelKnown && row.wsId >= 0
                                    text: "⟐"
                                    font.family: gadget.faceMono; font.pixelSize: 10
                                    color: row.idHue
                                }
                                Text {                     // which agent conducts this shell
                                    id: condTag
                                    anchors.baseline: elapsedText.baseline
                                    visible: row.conductedBy.length > 0
                                    text: "⇢ " + row.conductedBy
                                    font.family: gadget.faceMono; font.pixelSize: 10
                                    color: row.idHue
                                }
                                Text {                     // the Hyprland workspace — plain number tag;
                                                            // the trailing field for a main agent (after
                                                            // "⟐") or a shell (after elapsed/conductor).
                                                            // A subagent has no workspace of its own — its
                                                            // trailing field is the type tag instead.
                                    id: wsTag
                                    anchors.baseline: elapsedText.baseline
                                    visible: row.wsId >= 0 && !row.subagent
                                    text: "ws" + row.wsId
                                    font.family: gadget.faceMono; font.pixelSize: 10
                                    color: row.idHue
                                }
                            }
                            // ACTIVITY — the current command/tool, tinted like the
                            // state; the "what is it doing" line. Hidden when absent.
                            Text {
                                width: parent.width
                                visible: row.activityText.length > 0
                                text: "▸ " + row.activityText
                                elide: Text.ElideRight
                                font.family: gadget.faceMono; font.pixelSize: 10
                                color: gadget.withA(row.accent, 0.95)
                            }
                            // SAY — the agent's latest words, tail-read from its
                            // transcript by the bridge; dim quoted prose, up to two
                            // lines. The "what is it saying" line under the tool.
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
                                    // A FIXED box, right-aligned: the frames of a
                                    // set differ in width on purpose (that width
                                    // change is the movement), and an auto-sized
                                    // Text would drag cwdText's elide around with
                                    // it every frame.
                                    width: 96
                                    horizontalAlignment: Text.AlignRight
                                    // WORKING animates; every resting state holds
                                    // one pose. Every row's set is HASHED from a
                                    // pin key (pickFor/phaseFor), never drawn at
                                    // random — the underlying roster model gets
                                    // reassigned (and every delegate recreated)
                                    // on ordinary activity/say heartbeats, far
                                    // more often than the roster actually
                                    // changes, so a random pick would visibly
                                    // reshuffle mid-session. A SUBAGENT wears the
                                    // `packages` courier pool, pinned to its own
                                    // id for its whole life (a consistent
                                    // delivery story). A top-level row that
                                    // currently has an active subagent child
                                    // wears the `receiving` pool (catch/inbox) —
                                    // pinned to id+":subs", so the set changes
                                    // (deliberately) only when it transitions
                                    // into/out of coordinating subagents. Every
                                    // other row is a plain solo agent, pinned to
                                    // its own id in the general `working` pool.
                                    property bool packageRow: row.subagent
                                    property bool receivingRow: !packageRow
                                        && gadget.hasActiveSubagents(modelData.sessionId || "")
                                    readonly property string moodKey: (modelData.sessionId || "")
                                        + (receivingRow ? ":subs" : "")
                                    readonly property var moodPool: packageRow ? gadget.faces.packages
                                        : (receivingRow ? gadget.faces.receiving : gadget.faces.working)
                                    property int setIdx: gadget.faces.pickFor(moodKey, moodPool)
                                    property int frame: gadget.faces.phaseFor(moodKey, frames.length)
                                    readonly property var frames: gadget.faces.workingFrames(setIdx, moodPool)
                                    text: row.working ? frames[frame % frames.length]
                                                      : gadget.kaomojiFor(modelData.state)
                                    // Explicit family (every other Text in this
                                    // row sets one too): the `packages` pool's
                                    // parcel glyph lives in the Nerd Font's
                                    // private-use icon range, which only
                                    // resolves against a Nerd Font — Qt's CJK/
                                    // kana fallback (relied on everywhere else
                                    // in this file) has no claim on that range.
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
                        }

                        // If this row is (re)created while it is the hovered row —
                        // e.g. a genuine roster change rebuilt the delegate under a
                        // stationary pointer — re-assert its workspace highlight so
                        // the bar preview persists without waiting for a mouse move.
                        Component.onCompleted: {
                            if (gadget.shared && gadget.shared.hoveredSessionId === (modelData.sessionId || ""))
                                gadget.setHover(modelData.sessionId || "",
                                                modelData.workspace !== undefined ? modelData.workspace : -1);
                        }

                        MouseArea {
                            id: hover
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            // hovering a roster row lights up that session's
                            // workspace on the bar (shared.hoveredWorkspace →
                            // WorkspaceRow preview highlight). Keyed on sessionId and
                            // cleared through the deferred guard so a roster refresh
                            // can't strand or wipe a hover the pointer still holds.
                            onEntered: gadget.setHover(modelData.sessionId || "",
                                                       modelData.workspace !== undefined ? modelData.workspace : -1)
                            onExited:  gadget.requestHoverClear(modelData.sessionId || "")
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
