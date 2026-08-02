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
    property string projectsPath: "/home/khoa/Aoide/song/stage/projects.json"

    implicitWidth: 360
    implicitHeight: 520

    // type voices ──────────────────────────────────────────────────────────────
    readonly property string faceSerif: "Noto Serif"                // carved marble
    readonly property string faceMono:  "JetBrainsMono Nerd Font"   // the terminal
    readonly property string faceMusic: "Noto Music"                // notation

    // live state ────────────────────────────────────────────────────────────────
    property var sessions: []              // raw roster (every stage record)
    property var projects: []              // raw project roster ({name, path})
    property var visibleRows: []           // filtered/grouped roster actually drawn:
                                            // project-header rows + session rows
    property var agentIndex: ({})           // effective-agent name → identity slot
    property real nowMs: Date.now()
    property string emphId: ""
    property int projectCount: 0           // distinct ANCHORED projects with ≥1 session
    property int sessionCount: 0           // total sessions in the roster (fold-independent)
    // Fleet-wide context rollup (over EVERY session, regardless of fold state
    // — a collapsed project still counts toward the fleet total).
    property bool fleetHasCtx: false
    property real fleetMaxFill: 0
    property real fleetSumTok: 0
    // Per-project fold state: projectKey → collapsed(bool). Reassigned (never
    // mutated in place) on toggle so the property-change + recompute() fire.
    property var collapsed: ({})
    // Sentinel bucket key for sessions that anchor to no project — a NUL byte
    // can never appear in a real filesystem path, so it can't collide with a
    // project's own `path` key.
    readonly property string unanchoredKey: "\u0000unanchored"
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

    // A signature over ONLY the fields the delegate renders — so a rewrite that
    // touches nothing visible (a heartbeat timestamp, a socket path) yields the
    // same string and we keep the existing delegates (and the live hover) intact.
    // Extended for project grouping: a "project-header" row folds in its
    // fold state + rollup, and a "session" row folds in its bucket key -- so
    // re-anchoring (a projects.json edit) or a fold toggle both yield a
    // different signature and swap the model, while a no-op heartbeat still
    // does not. Keeps the pre-existing convention of control-byte field/row
    // separators (0x1f/0x01) rather than a printable one, so no session
    // field value can ever collide with the delimiter.
    function rosterSig(rows) {
        var parts = [];
        for (var i = 0; i < rows.length; i++) {
            var r = rows[i];
            if (r._kind === "project-header") {
                parts.push(["H", r._projectKey, r._collapsed, r._live, r._total,
                            r._hasCtx, r._maxFill, r._sumTok].join(""));
            } else {
                parts.push(["S", r.sessionId, r.agent, r.state, r.cwd, r.startedAt,
                            r.workspace, r._conductedBy, r.title, r.activity, r.say,
                            r.kind, r.parentSessionId, r._depth, r._parentAgent,
                            r.model, r.contextTokens, r._projectKey].join(""));
            }
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
        if (parts.length <= 4) return p;
        return "…/" + parts.slice(-4).join("/");
    }

    // ── PROJECT ANCHORING (mirrors pkgs/aoide/src/graph/model.rs) ────────────────
    // The anchoring project for `cwd`: the LONGEST matching project root wins
    // (so a nested project anchors to the deeper root), where a match is
    // path-component-aware — `cwd === root` or `cwd.startsWith(root + "/")` —
    // with a trailing "/" trimmed off multi-char roots before the compare. A
    // faithful port of `cwd_under`/`anchor_for` (model.rs ~lines 222-240):
    // same conditional trim for the match test, same UNconditional trim for
    // the tie-break length (mirrors Rust's own asymmetry there). Sessions
    // under no project root return null (unanchored).
    function anchorProject(cwd) {
        if (!cwd) return null;
        var best = null, bestLen = -1;
        for (var i = 0; i < projects.length; i++) {
            var p = projects[i];
            var root = p.path || "";
            var matchRoot = (root.length > 1) ? root.replace(/\/+$/, "") : root;
            var under = (cwd === matchRoot) || (cwd.indexOf(matchRoot + "/") === 0);
            if (!under) continue;
            var tieLen = root.replace(/\/+$/, "").length;   // unconditional trim (matches Rust's max_by_key key)
            if (tieLen > bestLen) { best = p; bestLen = tieLen; }
        }
        return best;
    }

    // ── PROJECT GROUPING ──────────────────────────────────────────────────────
    // Buckets the beamed tree (computeVisible()'s flat roots+children array,
    // _depth 0/1) by the project its ROOT anchors to. A depth-1 child
    // INHERITS its root's bucket rather than being anchored by its own cwd —
    // subagent records routinely carry no cwd of their own, and splitting a
    // subtree across groups would sever the beamed-tree nesting the rest of
    // this file goes to some trouble to preserve. Relies on computeVisible's
    // walk() ordering: every root's depth-1 descendants are emitted
    // immediately after it, before the next root, so a single linear pass
    // (reset the "current" project only at depth 0) is sufficient. Returns
    // the group keys in render order — alphabetical by project name, with
    // the unanchored bucket (if non-empty) trailing — plus a key→bucket map.
    function groupProjects(tree) {
        var byKey = ({}), order = [];
        var curKey = "", curName = "";
        for (var i = 0; i < tree.length; i++) {
            var r = tree[i];
            if ((r._depth || 0) === 0) {
                var proj = anchorProject(r.cwd || "");
                curKey = proj ? proj.path : unanchoredKey;
                curName = proj ? proj.name : "";
            }
            r._projectKey = curKey;
            if (!byKey[curKey]) { byKey[curKey] = { name: curName, rows: [] }; order.push(curKey); }
            byKey[curKey].rows.push(r);
        }
        var named = order.filter(function (k) { return k !== unanchoredKey; });
        named.sort(function (a, b) {
            var na = (byKey[a].name || "").toLowerCase(), nb = (byKey[b].name || "").toLowerCase();
            return na < nb ? -1 : (na > nb ? 1 : 0);
        });
        if (byKey[unanchoredKey]) named.push(unanchoredKey);
        return { keys: named, byKey: byKey };
    }

    // Flattens the grouped buckets into the single ListView model: one
    // "project-header" row per bucket, then that bucket's "session" rows
    // (the pre-existing beamed tree, untouched) — omitted while the bucket
    // is folded, so a collapsed project still shows its header (and thus its
    // rollup — a folded near-full project keeps flagging itself).
    function buildVisibleRows(grouped) {
        var out = [];
        for (var i = 0; i < grouped.keys.length; i++) {
            var key = grouped.keys[i];
            var bucket = grouped.byKey[key];
            var rollup = notes.ctxRollup(bucket.rows);
            var live = 0, j;
            for (j = 0; j < bucket.rows.length; j++) {
                var st = bucket.rows[j].state;
                if (isWorking(st) || isAwaiting(st)) live++;
            }
            var isUnanchored = key === unanchoredKey;
            var folded = !!collapsed[key];
            out.push({
                _kind: "project-header",
                _projectKey: key,
                _label: isUnanchored ? "(unanchored)" : bucket.name,
                _unanchored: isUnanchored,
                _collapsed: folded,
                _live: live,
                _total: bucket.rows.length,
                _hasCtx: rollup.any,
                _maxFill: rollup.maxFill,
                _sumTok: rollup.sumTok
            });
            if (!folded) {
                for (j = 0; j < bucket.rows.length; j++) {
                    var rr = bucket.rows[j];
                    rr._kind = "session";
                    out.push(rr);
                }
            }
        }
        return out;
    }

    // MouseArea target for a project-header row: flips its fold state. The
    // map is REASSIGNED (never mutated in place) so the property-change
    // signal fires, then recompute() re-derives visibleRows synchronously —
    // the same imperative "mutate, then recompute()" shape as every other
    // roster-affecting action in this file (compare setHover/parseStage).
    function toggleProject(key) {
        var c = {};
        for (var k in collapsed) c[k] = collapsed[k];
        c[key] = !c[key];
        collapsed = c;
        recompute();
    }

    function recompute() {
        var tree = computeVisible();
        // assign each distinct effective-agent a stable identity slot (order of
        // first appearance), so hueFor() cycles the base16 spread deterministically.
        // Computed over the FULL tree (fold-independent) so hue assignment never
        // shuffles just because a project got folded/unfolded.
        var idx = ({}), slot = 0, i;
        for (i = 0; i < tree.length; i++) {
            var k = effAgent(tree[i]);
            if (k.length > 0 && idx[k] === undefined) { idx[k] = slot; slot++; }
        }
        agentIndex = idx;
        emphId = computeEmph(tree);
        sessionCount = tree.length;

        var grouped = groupProjects(tree);
        var pc = 0;
        for (i = 0; i < grouped.keys.length; i++)
            if (grouped.keys[i] !== unanchoredKey) pc++;
        projectCount = pc;

        var fleet = notes.ctxRollup(tree);
        fleetHasCtx = fleet.any;
        fleetMaxFill = fleet.maxFill;
        fleetSumTok = fleet.sumTok;

        var next = buildVisibleRows(grouped);
        // Only reassign the ListView model when the rendered roster actually
        // changed. agentIndex/emphId/projectCount/sessionCount/fleet* above are
        // plain properties whose re-evaluation does NOT recreate delegates;
        // `visibleRows` is the model, so holding its reference stable across
        // no-op reloads is what stops the flicker.
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
    function parseProjects() {
        try {
            var t = projStage.text();
            if (!t || t.trim().length === 0) { projects = []; return; }
            var o = JSON.parse(t);
            projects = (o && o.projects) ? o.projects : [];
        } catch (e) {
            projects = [];
        }
    }
    onSessionsChanged: recompute()
    onProjectsChanged: recompute()

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
    // the project roster — hot-reloads on change, tolerates a missing/empty
    // file (song/stage/projects.json may not exist yet on a fresh rig).
    FileView {
        id: projStage
        path: gadget.projectsPath
        watchChanges: true
        blockLoading: false
        printErrors: false
        onLoaded: gadget.parseProjects()
        onFileChanged: reload()
    }
    Timer { interval: 1000; running: true; repeat: true; onTriggered: gadget.nowMs = Date.now() }
    Component.onCompleted: { parseStage(); parseProjects(); }

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
                    // FLEET rollup: the level ABOVE the project header — same
                    // two numbers (max-fill, token sum) over the ENTIRE roster
                    // regardless of fold state (a folded project's sessions
                    // still count here). Uses gadget.sessionCount (the full
                    // tree, not gadget.visibleRows.length, which now also
                    // counts project-header rows). The "· ctx …" clause is
                    // omitted entirely — not rendered as an empty gauge — until
                    // some session has produced a turn (gadget.fleetHasCtx).
                    id: ffL
                    anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter
                    text: "└─┤ " + gadget.sessionCount + " sess"
                          + " · " + gadget.projectCount + " proj"
                          + (gadget.fleetHasCtx
                             ? " · ctx " + notes.ctxBar(gadget.fleetMaxFill, 6) + " "
                               + Math.round(gadget.fleetMaxFill) + "%"
                               + (gadget.fleetMaxFill >= notes.ctxUrgentAt ? "!" : "")
                               + " · " + notes.ctxCompact(gadget.fleetSumTok)
                             : "")
                          + " ├"
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

                    // Dispatches each row on modelData._kind: a project-header
                    // row (new) or a plain session row (the pre-existing
                    // beamed-tree delegate, body unchanged below). Both
                    // Components are declared as CHILDREN of rowRoot — not
                    // siblings of the ListView — so their creation context
                    // chains through rowRoot and modelData stays resolvable
                    // inside either one (a Loader whose sourceComponent points
                    // at an externally-declared Component would NOT see the
                    // per-row modelData/index context properties).
                    delegate: Item {
                        id: rowRoot
                        width: roster.width
                        height: rowLoader.item ? rowLoader.item.height : 0

                        Loader {
                            id: rowLoader
                            width: parent.width
                            sourceComponent: modelData._kind === "project-header"
                                ? projectHeaderComponent : sessionRowComponent
                        }

                    Component {
                        id: sessionRowComponent
                        Item {
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
                        // nests every session row one indent under its project
                        // header (root or child alike), so the roster reads as
                        // header, then its own subtree -- not a flat list.
                        readonly property int groupIndent: 14
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
                        // blocked on a `sudo` password prompt — a conducted
                        // SHELL only, but read off whatever record the row
                        // wears (a plain agent/subagent record simply never
                        // carries this key). Distinct from ordinary `awaiting`
                        // ("the agent wants a permission answer"): this reads
                        // as "it's YOUR terminal password".
                        readonly property bool needsSudo: modelData.needsSudo === true

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
                            anchors.left: parent.left; anchors.leftMargin: 18 + row.groupIndent
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
                            anchors.left: parent.left; anchors.leftMargin: 12 + row.indent + row.groupIndent
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
                                    // The state tag + lock badge claim their space
                                    // FIRST (pinned to the row's right edge below)
                                    // and the name elides into whatever is left — a
                                    // fixed reserve let a long title push either tag
                                    // off the row's right edge.
                                    width: Math.max(24, Math.min(implicitWidth,
                                                    body.width - stateTag.slot - sudoBadge.slot))
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
                                Text {                     // SUDO lock badge — distinct from the
                                                            // ordinary awaiting state tag: this row
                                                            // needs the user's TERMINAL password, not
                                                            // an agent permission answer. A MONOCHROME
                                                            // nerd-font lock (not the 🔒 emoji glyph —
                                                            // Qt ignores Text.color on color-emoji
                                                            // glyphs, so it never followed the palette)
                                                            // rendered in the same mono/nerd face as the
                                                            // rest of the terminal furniture, so it DOES
                                                            // honor notes.paletteUrgent.
                                    id: sudoBadge
                                    readonly property real slot: visible ? implicitWidth + 6 : 0
                                    anchors.right: stateTag.left
                                    anchors.rightMargin: visible ? 6 : 0
                                    anchors.baseline: agentName.baseline
                                    visible: row.needsSudo
                                    text: ""           //  nf-fa-lock
                                    font.family: gadget.faceMono
                                    font.pixelSize: 13
                                    color: notes.paletteUrgent
                                    // A quicker ping than the row-wide awaiting
                                    // wash (560/640ms below) — the badge itself
                                    // reads as urgent even before the eye finds
                                    // the wash.
                                    SequentialAnimation on opacity {
                                        running: row.needsSudo
                                        loops: Animation.Infinite; alwaysRunToEnd: true
                                        onRunningChanged: if (!running) sudoBadge.opacity = 1.0
                                        NumberAnimation { to: 0.35; duration: 380; easing.type: Easing.InOutSine }
                                        NumberAnimation { to: 1.0;  duration: 380; easing.type: Easing.InOutSine }
                                    }
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
                            // TALLY + MOOD — the row's metadata line, composed into
                            // two distinct grammars (a shell keeps the third, older
                            // one), sharing the line with the animated mood face.
                            // Both separators are drawn from the widget's OWN glyph
                            // grammar rather than a generic UI arrow — "·" (the ano
                            // teleia) reads as the subordinate, ECHOING voice a
                            // subagent is; "⟐" is the diamond that used to sit as
                            // the model tag's own prefix, repurposed here as the
                            // join it was always halfway to being, for the PRIMARY
                            // (main-agent) line — the two are shape- and meaning-
                            // distinct at a glance, not just different chars:
                            //   subagent:   elapsed  model · type       ("·" joins
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
                            // rides directly above the cwd line, paired with the
                            // mood face on its right (the face's spot is unchanged —
                            // it just now shares the line with its own caption
                            // instead of sitting astride the cwd path). The meta
                            // tags pack a Row so an invisible tag never leaves a gap.
                            Item {
                                width: parent.width
                                height: Math.max(metaRow.implicitHeight, kao.implicitHeight)
                                Row {
                                    id: metaRow
                                    anchors.left: parent.left
                                    anchors.right: kao.left; anchors.rightMargin: 6
                                    anchors.verticalCenter: parent.verticalCenter
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
                                    Text {                     // CONTEXT-WINDOW METER — bar + percent +
                                                                // compact count, in the dock's existing
                                                                // `[▓░]` ASCII-gauge grammar (AoideBar
                                                                // battBar / MetersGadget barFill). Sits
                                                                // right after the model tag it describes.
                                                                // Zero footprint until the session has
                                                                // produced an assistant turn: this Row
                                                                // (like the rest of the tally) skips
                                                                // invisible children entirely, so an
                                                                // absent contextTokens never reflows the
                                                                // row — the same effect the sudoBadge/
                                                                // stateTag slot dance achieves above for
                                                                // the (anchored, non-Row) name line.
                                        id: ctxTag
                                        anchors.baseline: elapsedText.baseline
                                        visible: (modelData.contextTokens || 0) > 0
                                        readonly property real pct: notes.ctxPercent(modelData.model, modelData.contextTokens)
                                        text: notes.ctxBar(pct, 6) + " " + Math.round(pct) + "% · " + notes.ctxCompact(modelData.contextTokens)
                                        font.family: gadget.faceMono; font.pixelSize: 10
                                        // song accent (this row's identity hue) → paletteUrgent
                                        // past ~85%, same threshold/swap as the sudo badge's
                                        // urgency grammar and MetersGadget's CPU/RAM gauges.
                                        color: notes.ctxColor(pct, row.idHue)
                                    }
                                    Text {                     // subagent separator — joins the model to
                                                                // the type tag; only when both are present.
                                                                // The ano teleia (Greek high dot): a quiet
                                                                // join, distinct from the main agent's "⟐".
                                        id: subSep
                                        anchors.baseline: elapsedText.baseline
                                        visible: row.subagent && row.modelKnown && row.showType
                                        text: "·"
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
                                                                // subagent's "·" above).
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
                                    Text {                     // the Hyprland workspace — plain number tag,
                                                                // mirroring TerminalsGadget's ws tag idiom
                                                                // (same guard, same withA(hue, 0.85) tint).
                                        id: wsTag
                                        anchors.baseline: elapsedText.baseline
                                        visible: row.wsId >= 0
                                        text: "ws" + row.wsId
                                        font.family: gadget.faceMono; font.pixelSize: 10
                                        color: gadget.withA(row.idHue, 0.85)
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
                            // the DIR — the session's working directory, as subtext.
                            // Alone on its own line now that the meta tags and the
                            // mood face share the line above it.
                            Text {
                                id: cwdText
                                width: parent.width
                                elide: Text.ElideMiddle
                                text: gadget.shortCwd(modelData.cwd)
                                font.family: gadget.faceMono; font.pixelSize: 10
                                color: gadget.withA(notes.holoBlue, 0.95)
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
                        }   // close: Item { id: row (the session-row delegate body)
                    }       // close: Component { id: sessionRowComponent

                    // ── PROJECT-HEADER row: ▾/▸ fold glyph · project name ·
                    // [live/total] badge · the max-fill/token-sum rollup gauge.
                    // Styled as a course of the same masonry the row ledger
                    // lines already read as — a deeper marble sub-band closed
                    // by an accent-toned rule (not the session rows' wireCyan
                    // ledger, so a header reads apart from a note at a glance)
                    // — rather than a foreign flat-UI list header.
                    Component {
                        id: projectHeaderComponent
                        Item {
                            id: hdr
                            width: roster.width
                            height: 26

                            readonly property bool folded: modelData._collapsed === true
                            readonly property bool hasCtx: modelData._hasCtx === true
                            readonly property real fillPct: modelData._maxFill || 0
                            readonly property color rollupColor: notes.ctxColor(fillPct, notes.violet)

                            Rectangle {                    // deeper marble sub-band — echoes
                                                            // the entablature's own band so this
                                                            // reads as a course of the stele, not
                                                            // a flat-UI list header; a touch
                                                            // brighter on hover (the fold target).
                                anchors.fill: parent
                                color: gadget.withA(notes.paletteFg, hoverArea.containsMouse ? 0.09 : 0.055)
                            }
                            Rectangle {                    // closing rule — accent-toned, distinct
                                                            // from the session rows' wireCyan ledger
                                                            // line, so a header reads apart at a glance.
                                anchors.bottom: parent.bottom
                                width: parent.width; height: 1
                                color: gadget.withA(notes.paletteAccent, 0.35)
                            }

                            Text {                         // fold glyph — ▾ open, ▸ folded
                                id: foldGlyph
                                anchors.left: parent.left; anchors.leftMargin: 12
                                anchors.verticalCenter: parent.verticalCenter
                                text: hdr.folded ? "▸" : "▾"
                                font.family: gadget.faceMono; font.pixelSize: 12
                                color: gadget.withA(notes.paletteAccent, 0.95)
                            }
                            Text {                         // project name — serif, matching the
                                                            // entablature's carved-inscription voice;
                                                            // the unanchored bucket wears it dimmed
                                                            // + italic instead of a project's name.
                                id: projName
                                anchors.left: foldGlyph.right; anchors.leftMargin: 6
                                anchors.verticalCenter: parent.verticalCenter
                                text: modelData._label
                                font.family: gadget.faceSerif
                                font.italic: modelData._unanchored === true
                                font.pixelSize: 13; font.weight: Font.DemiBold
                                color: modelData._unanchored ? gadget.withA(notes.paletteFg, 0.55) : notes.paletteFg
                            }
                            Text {                         // [live] — or [live/total] once they
                                                            // diverge (a done/idle session sits in
                                                            // the total but isn't "live").
                                id: countBadge
                                anchors.left: projName.right; anchors.leftMargin: 8
                                anchors.verticalCenter: parent.verticalCenter
                                text: "[" + (modelData._live === modelData._total
                                             ? modelData._total
                                             : (modelData._live + "/" + modelData._total)) + "]"
                                font.family: gadget.faceMono; font.pixelSize: 10
                                color: gadget.withA(notes.paletteFg, 0.55)
                            }
                            Text {                         // ROLLUP — max-fill bar + percent (the
                                                            // hottest window beneath this header,
                                                            // urgent past ctxUrgentAt) · raw token
                                                            // sum, compact. Omitted entirely (not an
                                                            // empty gauge) until some session in this
                                                            // project has produced a turn.
                                id: rollupTag
                                visible: hdr.hasCtx
                                anchors.right: parent.right; anchors.rightMargin: 12
                                anchors.verticalCenter: parent.verticalCenter
                                text: notes.ctxBar(hdr.fillPct, 6) + " " + Math.round(hdr.fillPct) + "%"
                                      + (hdr.fillPct >= notes.ctxUrgentAt ? "!" : "")
                                      + " · " + notes.ctxCompact(modelData._sumTok)
                                font.family: gadget.faceMono; font.pixelSize: 10
                                color: hdr.rollupColor
                            }

                            MouseArea {                    // click anywhere on the header to fold/
                                                            // unfold — same click-to-act feel as a
                                                            // session row's click-to-focus.
                                id: hoverArea
                                anchors.fill: parent
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                onClicked: gadget.toggleProject(modelData._projectKey)
                            }
                        }
                    }
                    }   // close: Item { id: rowRoot (the actual ListView delegate)
                }
            }
        }
    }
}
