// terminals.qml — sonata's "terminals" slot: THE TERMINALS, an IONIC temple
// in the dock's gadget column (WidgetSlot host — root is an Item that sizes
// itself off its content, same as every other slot in this directory).
//
// Ported from the facet's TerminalsGadget.qml (per-song widget-slot
// expansion, CONTRACTS.md §5) — ownership moved from the facet to sonata's
// score. Live: sonata's own `widgets/dock.qml` embeds
// `WidgetSlot { slot: "terminals" }` in its gadget column (slots.md's wired
// table). The facet's TerminalsGadget.qml stays in the tree but is no
// longer instantiated by anything.
//
import QtQuick
import Quickshell
import Quickshell.Hyprland
import Quickshell.Io
// Reaches the facet's shared components — MoodFaces (the kaomoji troupe)
// and ScrollRail (the draggable scrollbar) — neither of which is
// terminals-specific chrome, so they stay in the facet rather than moving
// here. Deployed-tree relative path: this file lands at $out/qml/songs/
// sonata/terminals.qml, so two levels up ($out/qml/songs/ → $out/qml/) is
// the facet's own qml/ root (bar.qml idiom; see that file's own header).
import "../.."

// ── THE TERMINALS ────────────────────────────────────────────────────────────
// A live view of the open terminal roster (the terminal-commander): every tty
// on the stage. A SIBLING of the Conductor in the same pantheon — but a
// different god's house. Where the Conductor is a Doric marble stele leaning
// gold, this is an IONIC temple leaning aegean (holoBlue): scroll-volute
// capital, a dentil cornice, an egg-and-dart rule, and square-cornered box
// framing — the ARCHITECTURE differs; the plaques inside it do not.
//
// SHARED across the pantheon (the family resemblance):
//   · opaque + DEFINED body — hard plum border, inset keyline, cast shadow;
//     no pale washout, fully legible. All colour from `livery` roles; radius 0.
//   · MUSIC state-glyph contract  ♪ working · 𝄐 awaiting · 𝄼 stopped · 𝄽 idle ·
//     𝄂 done · · unknown, closed by a final barline 𝄂. (glyphs = HARD CONTRACT)
//   · KAOMOJI every working row draws from MoodFaces.qml's general `working`
//     pool (terminals have no subagent concept, so the `packages` courier
//     pool never appears here), hashed from the row's stable key so its set
//     stays pinned across a delegate rebuild; ASCII / box-drawing throughout.
//   · the FUNCTION: agent · state · cwd · elapsed, click → focusSession,
//     an empty state, and hot-reload of the stage file.
//
// ORDER and FRAME (the same plaques, another god's house):
//   · ORDER   — Ionic, not Doric: a volute (scroll) capital + dentil course
//               instead of the entablature band + baton course.
//   · RULE    — egg-and-dart, not the solid baton course.
//   · SIGNATURE HUE — aegean holoBlue carries the architecture (the Conductor's
//               gold recedes to a small family nod on the tag).
//   · FRAME   — square box corners (┌ ┐ └ ┘), shared with the other temples.
//
// The ROWS wear the Conductor's card silhouette — the owner's round-2
// directive ("terminals should look like the conductor") retires the old
// ledger lines and fluted gutter columns for the Conductor's plaque grammar —
// with ONE deliberate carve-out, also the owner's: the AGENT-SPECIFIC INFO
// CORE (the screen pane, the [▓░] ctx meter, the plain elapsed tally) keeps
// this temple's OWN treatment. The Conductor gives every agent a dedicated
// card; this roster packs the same agent into one row among many — the
// information density is legitimately different, so that core is not forced
// pixel-identical. One plaque grammar at two scales:
//   · AGENT terminals — reserved 16px lamp box (metronome pulse, terracotta
//     breath, nf-fa-lock on sudo) · serif name · state word + ws tag corner /
//     the aegean SCREEN PANE ($ live command + model cell + three say lines) /
//     elapsed + [▓░] meter vitals / cwd + troupe ground.
//   · bare ttys — SUB-scale: lamp · mono command (a command, not a name) ·
//     the same corner / elapsed / cwd + troupe. No pane, no meter — agent-only
//     content; a Column skips invisible children.
// The plaque chrome is the Conductor's verbatim (fg-hairline 0.22, fill 0.035,
// urgent border on awaiting/sudo); the left pilaster speaks WORKSPACE note-hue
// where the Conductor speaks project (3px laurel crown when traced, same law).
// Constant-height per kind; the sanctioned growths: the ground cwd WRAPS when
// a real dir overflows (a yazi live dir is long and space-free — an elided
// line would hide which dir it is on), and the COMMAND lines (the bare label,
// the pane's prompt) wrap to a 3-line cap — a long command reads, never just
// truncates.
// The one laurel-`paletteHot` crown (the traced session) stays a pantheon-wide
// signal, identical across temples. The music state colours stay shared too —
// a "working" note reads the same in every house; only the architecture differs.
// Codex main agents share the Conductor's 24×18 illuminated book: pages turn
// while working, rest open on a hold, and close at idle/stopped/done.
// The identity row reserves its width through state changes; bare shells and
// subagents never acquire an agent badge from a title or foreground command.
Item {
    id: gadget

    required property var livery            // palette roles
    required property var bridge           // socket sender (bridge.focusSession)
    property var shared: null              // cross-widget state (shared.tracedSessionId)
    // sessions.json is a CONDUCTING file (CONTRACTS.md §4) — state/stage/,
    // not song/stage/.
    property string stagePath: (Quickshell.env("AOIDE_ROOT") || (Quickshell.env("HOME") + "/.aoide")) + "/state/stage/sessions.json"

    implicitWidth: 360
    implicitHeight: 520

    // type voices ──────────────────────────────────────────────────────────────
    readonly property string faceSerif: "Noto Serif"                // carved marble
    readonly property string faceMono:  "JetBrainsMono Nerd Font"   // the terminal
    readonly property string faceMusic: "Noto Music"                // notation

    // this temple's signature — aegean, not the Conductor's gold ─────────────────
    readonly property color sig: livery.holoBlue

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

    // Transform LISTS are fixed on this host and are not bindable in Qt.
    // Discover their objects at attachment; bind to each Translate's x/y below.
    function markTransforms(mark) {
        var out = []
        for (var node = mark; node; node = node.parent)
            for (var i = 0; i < node.transform.length; i++)
                out.push(node.transform[i])
        return out
    }

    // Item.visible stays true behind a clip or the dock's translated cover.
    // Snapshot the ancestor geometry explicitly: mapToItem() alone does not
    // register those dependencies for a QML binding. The current host's
    // transform list contains Translate; its x/y need their own reads too.
    // Kept identical in Conductor so both marks sleep behind the same clips.
    function markExposed(mark, win, shifts) {
        if (!mark || !mark.visible || !win || !win.visible) return false
        var positions = []
        for (var i = 0; i < shifts.length; i++)
            positions.push([shifts[i].x, shifts[i].y])
        var chain = []
        for (var node = mark; node; node = node.parent) {
            chain.push({ item: node, parent: node.parent,
                x: node.x, y: node.y, width: node.width, height: node.height,
                visible: node.visible, opacity: node.opacity, clip: node.clip,
                scale: node.scale, rotation: node.rotation,
                transformOrigin: node.transformOrigin })
        }
        var rect = Qt.rect(0, 0, mark.width, mark.height)
        for (var j = 0; j < chain.length; j++) {
            var g = chain[j]
            if (!g.visible || g.opacity <= 0) return false
            if (g.clip) {
                var left = Math.max(0, rect.x), top = Math.max(0, rect.y)
                var right = Math.min(g.width, rect.x + rect.width)
                var bottom = Math.min(g.height, rect.y + rect.height)
                if (right <= left || bottom <= top) return false
                rect = Qt.rect(left, top, right - left, bottom - top)
            }
            rect = g.item.mapToItem(g.parent, rect)
        }
        return rect.width > 0 && rect.height > 0
            && rect.x < win.width && rect.y < win.height
            && rect.x + rect.width > 0 && rect.y + rect.height > 0
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
        case "awaiting": return livery.paletteUrgent;
        case "working":  return livery.paletteAccent;
        case "stopped":  return livery.holoBlue;
        case "idle":     return livery.violet;
        case "done":     return livery.wireCyan;
        default:         return withA(livery.paletteFg, 0.45);
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
    // Rank within ONE window — a LIVE agent beats a `done` agent, which beats
    // the shell hosting them. A window can hold several agent records at once:
    // each /clear, compact or resume mints a new sessionId and leaves the
    // outgoing one `done` until the reaper drops it (see
    // `superseded_done_siblings` in reap.rs). Ranking only agent-over-shell —
    // as this did — made the winner whichever agent the file listed FIRST, so
    // the row rendered a dead session's state, cwd and say while a live agent
    // worked in that very terminal. Ties keep the first seen.
    function recRank(rec) {
        if (!isAgentRec(rec)) return 0;                        // the hosting shell
        return normState(rec.state) === "done" ? 1 : 2;        // tombstone < live agent
    }
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
            else if (recRank(s) > recRank(cur)) { byAddr[addr] = s; }  // live agent > tombstone > shell
        }
        var out = [];
        for (var k = 0; k < order.length; k++) {
            var rec = byAddr[order[k]];
            var wsId = (rec.workspace !== undefined && rec.workspace !== null) ? rec.workspace : -1;
            out.push({
                sessionId:     rec.sessionId || "",
                agent:         rec.agent || "shell",
                kind:          recKind(rec),          // preserve the published kind for paint predicates
                state:         rec.state || "idle",
                cwd:           rec.cwd || "",
                startedAt:     rec.startedAt || "",
                workspace:     wsId,
                windowAddress: rec.windowAddress || "",
                title:         rec.title || "",       // session name (tracked) / window title (synthetic)
                activity:      rec.activity || "",    // the current command/tool
                tool:          rec.tool || "",         // the reaper's transcript-read tool
                                                        // label — richer than activity but
                                                        // can lag one reap; see toolText below
                say:           rec.say || "",         // the agent's latest words
                model:          rec.model || "",         // the running Claude model, if known
                contextTokens:  rec.contextTokens || 0,  // context-window fill of the last request
                contextCeiling: rec.contextCeiling || 0, // the published ceiling for that model (CONTRACTS.md §4)
                needsSudo:      rec.needsSudo === true   // blocked on a sudo password prompt
                                                         // (was dropped here once, so the sudo
                                                         // lock + urgent border below never fired)
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
            parts.push([r.sessionId, r.agent, r.kind, r.state, r.cwd, r.startedAt,
                        r.workspace, r.windowAddress, r.title,
                        r.activity, r.tool, r.say, r.model, r.contextTokens,
                        r.contextCeiling, r.needsSudo].join(""));
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
        color: gadget.withA(livery.paletteFg, 0.22)
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
        color: livery.paletteBg
        border.color: livery.paletteFg
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
                    color: gadget.withA(livery.paletteFg, 0.05)
                }
                Row {
                    // the User, live, pointed at it directly: this was the only
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
                        // 26 (was 30): the bass clef is a squat, wide glyph —
                        // 26 optically matches the Conductor's treble at 24.
                        text: "𝄢"; font.family: gadget.faceMusic; font.pixelSize: 26
                        color: gadget.sig
                    }
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        text: "TERMINALS"
                        // the family inscription spec — serif 17 DemiBold ls4,
                        // same as CONDUCTOR's band (was 19/ls5, the one loud
                        // title in the pantheon).
                        font.family: gadget.faceSerif; font.pixelSize: 17
                        font.weight: Font.DemiBold; font.letterSpacing: 4
                        color: livery.paletteFg
                    }
                }
                Text {                                // terminal tag — a gold family
                                                      // nod that IS the recheck
                                                      // button (UsageGadget's hover-
                                                      // morph idiom, same as
                                                      // "[ conductor ]"): hovering
                                                      // swaps the inscription for
                                                      // the click cue in the same
                                                      // slot; a click fires an
                                                      // on-demand reap/rehook sweep
                                                      // instead of the daemon's
                                                      // ~12s timer. Guarded like
                                                      // focusSession — a no-op
                                                      // until the bridge command lands.
                    id: ttyTag
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    text: ttyMouse.containsMouse ? "[ reap ]" : "[ tty ]"
                    font.family: gadget.faceMono; font.pixelSize: 11
                    color: gadget.withA(livery.paletteAccent, 0.95)   // same weight as "[ conductor ]"
                    MouseArea {
                        id: ttyMouse
                        anchors.fill: parent; anchors.margins: -4
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: {
                            if (gadget.bridge && gadget.bridge.recheckSessions)
                                gadget.bridge.recheckSessions()
                        }
                    }
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

            // ── square-cornered top frame: ┌─┤ label ├────┐ ───────────────────
            Item {
                id: topFrame
                anchors.top: eggdart.bottom; anchors.topMargin: 8
                width: parent.width; height: 15

                Text {
                    id: tfL
                    anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter
                    text: "┌─┤ ♪ open ttys ├"
                    font.family: gadget.faceMono; font.pixelSize: 11
                    color: gadget.withA(gadget.sig, 0.95)
                }
                Text {
                    id: tfR
                    anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                    text: "┐"
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

            // ── square-cornered bottom frame: └─ tally ─── 𝄂 ┘ ───────────────
            Item {
                id: footer
                anchors.bottom: parent.bottom
                width: parent.width; height: 20

                Text {
                    id: ffL
                    anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter
                    text: "└─┤ " + gadget.rows.length + " terminal" + (gadget.rows.length === 1 ? "" : "s")
                          + " · " + gadget.projectCount + " cwd" + (gadget.projectCount === 1 ? "" : "s") + " ├"
                    font.family: gadget.faceMono; font.pixelSize: 11
                    color: gadget.withA(livery.paletteFg, 0.8)
                }
                Text {
                    id: ffCorner
                    anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter
                    text: "┘"
                    font.family: gadget.faceMono; font.pixelSize: 11
                    color: gadget.withA(gadget.sig, 0.95)
                }
                Text {                                // final barline closes the score
                    id: ffBar
                    anchors.right: ffCorner.left; anchors.rightMargin: 4
                    anchors.verticalCenter: parent.verticalCenter
                    anchors.verticalCenterOffset: -1
                    text: "𝄂"
                    font.family: gadget.faceMusic; font.pixelSize: 16   // the family closing-barline size
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
                    // the family TACET ritual — face, inscription, and caption
                    // at the Conductor's exact empty-stage spec (mono 15 @ 0.5,
                    // serif 14 ls6 @ 0.45, serif-italic 10 @ 0.4); only the
                    // fluted-column figure above stays this temple's own.
                    Text {
                        anchors.horizontalCenter: parent.horizontalCenter
                        text: "ᕕ( ᐛ )ᕗ"
                        font.family: gadget.faceMono; font.pixelSize: 15
                        color: gadget.withA(livery.paletteFg, 0.5)
                    }
                    Text {
                        anchors.horizontalCenter: parent.horizontalCenter
                        text: "TACET"
                        font.family: gadget.faceSerif; font.pixelSize: 14
                        font.letterSpacing: 6
                        color: gadget.withA(livery.paletteFg, 0.45)
                    }
                    Text {
                        anchors.horizontalCenter: parent.horizontalCenter
                        text: "no terminals on the stage"
                        font.family: gadget.faceSerif; font.italic: true
                        font.pixelSize: 10
                        color: gadget.withA(livery.paletteFg, 0.4)
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
                        // ONE plaque silhouette — the Conductor's card law worn
                        // at two sizes (see the ROWS note in the header): an
                        // AGENT terminal is a main plaque (identity · screen
                        // pane · vitals · ground — the pane + [▓░] vitals being
                        // the owner's carve-out, this temple's own info core),
                        // a bare tty the sub-scale plaque (identity · vitals ·
                        // ground). Fixed lanes per kind — live data streaming
                        // in never moves either. The sanctioned growths: a cwd
                        // long enough to wrap (yazi's live dir — the dir must
                        // stay readable) and the command lines' 3-line cap.
                        height: body.implicitHeight + 11   // 5 top + text + 6 base (card law)

                        // an emph row must have a real sessionId — plain untracked
                        // terminals ("" id) never claim the traced crown.
                        property bool emph: (modelData.sessionId || "") !== ""
                                            && modelData.sessionId === gadget.emphId
                        property bool awaiting: gadget.isAwaiting(modelData.state)
                        property bool working:  gadget.isWorking(modelData.state)
                        property color accent: emph ? livery.paletteHot
                                                    : gadget.stateColor(modelData.state)
                        // the roster's ONE structural fork: an AGENT terminal
                        // (claude/kimi/pi… — recKind falls back to agent!="shell")
                        // vs a bare tty. Agents wear the serif nameplate + model
                        // byline + screen pane + context meter; bare ttys stay
                        // slim ledger lines.
                        readonly property bool agentRow: gadget.isAgentRec(modelData)
                        readonly property bool codexMain: gadget.recKind(modelData) === "agent"
                            && ("" + (modelData.agent || "")).toLowerCase() === "codex"
                        readonly property bool codexLive: row.codexMain && row.working
                            && !row.rowAwaiting
                        readonly property string activityText: modelData.activity || ""
                        // the tool lane — what the agent last reached for. TWO
                        // sources, and they disagree on purpose: `activity` is
                        // hook-set the instant a tool starts but is only ever
                        // its bare NAME ("Bash"), and is cleared when the turn
                        // settles; `tool` is read off the transcript by the
                        // reaper, so it carries the subject ("Bash: cargo
                        // test") and SURVIVES the settle, but can sit one reap
                        // (~12s) behind. Same call → take the rich label; a
                        // live tool the transcript hasn't caught up to → take
                        // the live name. (mirrors ConductorGadget's SessionCard)
                        readonly property string liveTool: modelData.activity || ""
                        readonly property string lastTool: modelData.tool || ""
                        readonly property string toolText: {
                            if (liveTool === "") return lastTool
                            var a = liveTool.toLowerCase(), b = lastTool.toLowerCase()
                            return (b === a || b.indexOf(a + ":") === 0) ? lastTool : liveTool
                        }
                        // whether the ground line shows a REAL directory (the
                        // daemon's cwd read) or the title fallback (a window
                        // title — which for a yazi/editor tty is the COMMAND,
                        // not a path). Real dirs WRAP; fallback text stays a
                        // single middle-elided line, never wrapped.
                        readonly property bool cwdReal: (modelData.cwd || "") !== ""
                        // a BARE tty's main label: the foreground command / file
                        // being edited (`nvim livery.md`, `cargo test`), or the
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
                        readonly property color wsTagColor: gadget.livery.noteColor(row.wsId)
                        // blocked on a `sudo` password prompt — distinct from
                        // ordinary `awaiting` ("an agent permission answer"):
                        // this reads as "it's YOUR terminal password".
                        readonly property bool needsSudo: modelData.needsSudo === true
                        // the Conductor's card-state law: a sudo hold counts as
                        // awaiting (it drives the breath + the urgent border),
                        // and a resting row dims its lamp + state word to 0.55.
                        readonly property bool rowAwaiting: row.awaiting || row.needsSudo
                        readonly property bool resting: !row.working && !row.rowAwaiting
                        readonly property bool hasCtx: (modelData.contextTokens || 0) > 0
                        readonly property real ctxPct: row.hasCtx
                            ? livery.ctxPercent(modelData.contextTokens, modelData.contextCeiling) : 0

                        // Re-assert the bar highlight if this row is rebuilt while
                        // it is the hovered one (roster refresh under a still pointer).
                        Component.onCompleted: {
                            if (gadget.shared && gadget.shared.hoveredSessionId !== ""
                                && gadget.shared.hoveredSessionId === gadget.rowKey(modelData))
                                gadget.setHover(gadget.rowKey(modelData),
                                                modelData.workspace !== undefined ? modelData.workspace : -1);
                        }

                        // the row plaque — the Conductor's card chrome verbatim
                        // (fill 0.035, hover 0.10, laurel 0.05, fg-hairline
                        // border 0.22, awaiting/sudo border urgent 0.75); only
                        // the hover wash stays keyed to this temple's aegean.
                        Rectangle {
                            anchors.fill: parent
                            radius: 0
                            color: hover.containsMouse
                                   ? gadget.withA(gadget.sig, 0.10)
                                   : (row.emph ? gadget.withA(livery.paletteHot, 0.05)
                                               : gadget.withA(livery.paletteFg, 0.035))
                            border.width: 1
                            border.color: row.rowAwaiting
                                          ? gadget.withA(livery.paletteUrgent, 0.75)
                                          : gadget.withA(livery.paletteFg, 0.22)
                        }
                        // the pilaster — the Conductor's project stripe, spoken
                        // in this temple's own identity hue: the row's WORKSPACE
                        // note colour (the same grammar the ws tag wears; safe
                        // for id <= 0). The traced row wears the 3px laurel
                        // crown, same one-standout law as the Conductor plaque.
                        Rectangle {
                            x: 0; width: row.emph ? 3 : 2
                            height: parent.height
                            color: row.emph ? livery.paletteHot
                                            : gadget.withA(row.wsTagColor, 0.6)
                        }

                        Column {
                            id: body
                            anchors.left: parent.left; anchors.leftMargin: 9
                            anchors.right: parent.right; anchors.rightMargin: 6
                            anchors.top: parent.top; anchors.topMargin: 5
                            spacing: 3

                            // ── 1 · IDENTITY — lamp · name ── state word · wsN ──
                            // The Conductor's inscription row: the reserved
                            // 16px lamp box (metronome pulse working, terracotta
                            // breath awaiting, the nf-fa-lock swap + 380ms ping
                            // on a sudo hold — one urgency grammar across both
                            // temples), the name owning the left run, and the
                            // catalogue corner reading right→left. No #NN here
                            // (the graph ordinal is a Conductor idea); the ws
                            // tag holds the outermost corner instead, wearing
                            // the ledger marks for specials (𝅘𝅥𝅱 magic / 𝄋
                            // scratch) in the bar's own note hue.
                            Item {
                                width: parent.width
                                // grows as the command label wraps (its 3-line
                                // cap); single-line rows hold the two family
                                // steps. Corner tags + lamp ride LINE ONE
                                // (baseline/top anchors), not the row centre,
                                // so a wrapped label never drags them down.
                                height: Math.max(row.agentRow ? 20 : 16,
                                                 procName.implicitHeight + 2)

                                Text {                     // ws tag — the catalogue corner
                                    id: wsTagT
                                    visible: row.wsId >= 0 || row.wsSpecial
                                    anchors.right: parent.right
                                    anchors.baseline: procName.baseline
                                    text: row.isMagic ? "𝅘𝅥𝅱 magic"
                                        : (row.isScratch ? "𝄋 scratch"
                                                         : ("ws" + row.wsId))
                                    font.family: gadget.faceMono; font.pixelSize: 9
                                    font.bold: row.wsSpecial   // the bar bolds its note glyphs too
                                    color: gadget.withA(row.wsTagColor, row.wsSpecial ? 0.95 : 0.9)
                                }
                                Text {                     // state word — the lamp's caption,
                                                            // same state + colour source as the
                                                            // glyph so the two never disagree
                                    id: stateWordT
                                    anchors.right: wsTagT.visible ? wsTagT.left : parent.right
                                    anchors.rightMargin: wsTagT.visible ? 8 : 0
                                    anchors.baseline: procName.baseline
                                    text: gadget.stateLabel(modelData.state)
                                    font.family: gadget.faceSerif; font.italic: true
                                    font.pixelSize: 10   // the family state-word size
                                    color: gadget.withA(gadget.stateColor(modelData.state),
                                                        row.resting ? 0.55 : 1.0)
                                }
                                Item {                     // the reserved lamp box
                                    id: lampBox
                                    x: 0
                                    y: row.agentRow ? 2 : 0   // seats on line one
                                    width: 16; height: 16

                                    Text {
                                        id: lamp
                                        anchors.centerIn: parent
                                        // sudo swaps the note for the SAME
                                        // nf-fa-lock the Conductor lamp wears
                                        // (escape-written — the raw PUA char is
                                        // invisible in editors and got lost once).
                                        text: row.needsSudo ? "\uf023" : gadget.glyphFor(modelData.state)
                                        font.family: row.needsSudo ? gadget.faceMono : gadget.faceMusic
                                        font.pixelSize: 12
                                        color: row.emph
                                               ? livery.paletteHot
                                               : gadget.withA(row.needsSudo ? livery.paletteUrgent
                                                                            : gadget.stateColor(modelData.state),
                                                              row.resting ? 0.55 : 1.0)

                                        // working — the metronome pulse, confined
                                        // to the box (the anchoring law)
                                        SequentialAnimation on scale {
                                            running: row.working
                                            loops: Animation.Infinite
                                            alwaysRunToEnd: true
                                            NumberAnimation { to: 1.35; duration: 520; easing.type: Easing.InOutSine }
                                            NumberAnimation { to: 1.0;  duration: 520; easing.type: Easing.InOutSine }
                                        }
                                        // awaiting — the terracotta breath; a sudo
                                        // hold quickens it to the family's 380ms
                                        // ping ("your password" beats faster than
                                        // "an agent question").
                                        SequentialAnimation on opacity {
                                            running: row.rowAwaiting
                                            loops: Animation.Infinite
                                            alwaysRunToEnd: true
                                            NumberAnimation { to: 0.35; duration: row.needsSudo ? 380 : 700; easing.type: Easing.InOutSine }
                                            NumberAnimation { to: 1.0;  duration: row.needsSudo ? 380 : 700; easing.type: Easing.InOutSine }
                                        }
                                    }
                                }
                                Text {                     // the MAIN label — two voices: an
                                                            // agent row wears its NAME carved in
                                                            // serif at the family's main size; a
                                                            // bare tty keeps the mono foreground
                                                            // command at the sub step (a command
                                                            // is a command, not a name). The
                                                            // command WRAPS to a 3-line cap —
                                                            // commands carry spaces, so plain
                                                            // Wrap breaks at them; past the cap
                                                            // the last line still elides. The
                                                            // identity row's height rides this.
                                    id: procName
                                    anchors.left: lampBox.right; anchors.leftMargin: 5
                                    anchors.top: parent.top
                                    anchors.topMargin: row.agentRow ? 1 : 0
                                    elide: Text.ElideRight
                                    wrapMode: Text.Wrap
                                    maximumLineCount: 3
                                    width: parent.width - 21
                                           - (wsTagT.visible ? wsTagT.implicitWidth + 8 : 0)
                                           - (stateWordT.implicitWidth + 8)
                                           - (row.codexMain ? codexTag.width + 6 : 0)
                                    text: row.agentRow ? (modelData.agent || "agent") : row.procText
                                    font.family: row.agentRow ? gadget.faceSerif : gadget.faceMono
                                    font.pixelSize: row.agentRow ? 14 : 12
                                    font.weight: row.emph ? Font.Bold : Font.Medium
                                    color: gadget.withA(livery.paletteFg, row.agentRow ? 1.0 : 0.85)
                                }
                                // Codex's illuminated book — a printed leaf turns while working,
                                // a still open spread on a hold, and a closed cover at rest.
                                // Both leaf faces share the settled page's text and geometry;
                                // the loop joins on the same fully printed spread.
                                Item {
                                    id: codexTag
                                    visible: row.codexMain
                                    x: procName.x + Math.min(procName.paintedWidth, procName.width) + 6
                                    y: 1
                                    width: 24; height: 18
                                    clip: true
                                    readonly property color ink: gadget.withA(gadget.stateColor(modelData.state), 0.95)
                                    readonly property color gold: gadget.livery.paletteAccent
                                    readonly property color paper: gadget.livery.paletteBg
                                    readonly property bool openBook: row.working || row.rowAwaiting
                                    readonly property bool turning: row.codexLive
                                    readonly property var hostWindow: QsWindow.window
                                    property var hostTransforms: []
                                    property bool componentReady: false
                                    Component.onCompleted: {
                                        hostTransforms = gadget.markTransforms(codexTag)
                                        componentReady = true
                                    }
                                    Component.onDestruction: componentReady = false
                                    // Attachment signals fire before the outer root id is available.
                                    onParentChanged: if (componentReady && gadget) hostTransforms = gadget.markTransforms(codexTag)
                                    onHostWindowChanged: if (componentReady && gadget) hostTransforms = gadget.markTransforms(codexTag)
                                    readonly property bool exposed: componentReady && !!gadget
                                        && gadget.markExposed(codexTag, hostWindow, hostTransforms)
                                    property real leaf: 0

                                    SequentialAnimation on leaf {
                                        running: codexTag.turning && codexTag.exposed
                                        loops: Animation.Infinite
                                        onStopped: codexTag.leaf = 0
                                        NumberAnimation { from: 0; to: 1; duration: 1100; easing.type: Easing.InOutCubic }
                                        PauseAnimation { duration: 450 }
                                    }
                                    Canvas {
                                        anchors.fill: parent
                                        property color ink: codexTag.ink
                                        property color gold: codexTag.gold
                                        property color paper: codexTag.paper
                                        property bool openBook: codexTag.openBook
                                        property bool turning: codexTag.turning
                                        property real leaf: codexTag.leaf
                                        onInkChanged: requestPaint()
                                        onGoldChanged: requestPaint()
                                        onPaperChanged: requestPaint()
                                        onOpenBookChanged: requestPaint()
                                        onTurningChanged: requestPaint()
                                        onLeafChanged: requestPaint()
                                        onWidthChanged: requestPaint()
                                        onHeightChanged: requestPaint()
                                        onPaint: {
                                            var ctx = getContext("2d")
                                            ctx.reset()
                                            ctx.strokeStyle = ink
                                            ctx.fillStyle = paper
                                            ctx.lineWidth = 1
                                            if (!openBook) {
                                                // The resting volume: hard cover, spine, page block,
                                                // and one small gilt lozenge, all inside the same slot.
                                                ctx.globalAlpha = 0.8
                                                ctx.fillRect(7, 2.5, 11, 13)
                                                ctx.strokeRect(7, 2.5, 11, 13)
                                                ctx.beginPath()
                                                ctx.moveTo(9, 2.5); ctx.lineTo(9, 15.5)
                                                ctx.moveTo(9, 13.5); ctx.lineTo(18, 13.5)
                                                ctx.stroke()
                                                ctx.strokeStyle = gold
                                                ctx.globalAlpha = 0.55
                                                ctx.beginPath()
                                                ctx.moveTo(13.5, 6); ctx.lineTo(15, 8)
                                                ctx.lineTo(13.5, 10); ctx.lineTo(12, 8)
                                                ctx.closePath(); ctx.stroke()
                                                return
                                            }

                                            // Every face uses this painter: the same outline and printed
                                            // strokes at rest, on the front of a leaf, and on its back.
                                            // The gutter is excluded here and stroked once after all pages.
                                            function page(side, lift, alpha) {
                                                var edgeX = 12 + 10 * side
                                                ctx.strokeStyle = ink
                                                ctx.lineWidth = 1
                                                ctx.globalAlpha = alpha
                                                ctx.beginPath()
                                                ctx.moveTo(12, 5)
                                                ctx.quadraticCurveTo(12 + 5 * side, 2 - 2 * lift,
                                                                     edgeX, 3.5 - 1.5 * lift)
                                                ctx.lineTo(edgeX, 14 - lift)
                                                ctx.quadraticCurveTo(12 + 5 * side, 12.5 - lift, 12, 15)
                                                ctx.fill()   // fill closes the gutter; the outline does not
                                                ctx.globalAlpha = 0.8 * alpha
                                                ctx.stroke()
                                                // Identical decorative text on both faces. Foreshortening
                                                // compresses the lines into the gutter as the page turns.
                                                ctx.globalAlpha = 0.3 * alpha * Math.abs(side)
                                                for (var row = 0; row < 3; row++) {
                                                    var y = 6 + row * 2.5
                                                    ctx.beginPath()
                                                    ctx.moveTo(12 + 3 * side, y + 0.7 - 0.6 * lift)
                                                    ctx.lineTo(12 + 8 * side, y - 1.6 * lift)
                                                    ctx.stroke()
                                                }
                                            }
                                            page(-1, 0, 1)
                                            page(1, 0, 1)

                                            if (turning && leaf > 0 && leaf < 1) {
                                                var angle = Math.PI * leaf
                                                var lift = Math.sin(angle)
                                                var side = Math.cos(angle)
                                                // Vanishing overlay weight at BOTH endpoints prevents a
                                                // translucent outline being double-painted at landing/reset.
                                                // Geometry and text also converge to the resting page exactly.
                                                var alpha = lift * lift
                                                page(side, lift, alpha)
                                                ctx.strokeStyle = gold
                                                ctx.beginPath()
                                                ctx.moveTo(12 + 10 * side, 3.5 - 1.5 * lift)
                                                ctx.lineTo(12 + 10 * side, 14 - lift)
                                                ctx.globalAlpha = 0.13 * lift * alpha
                                                ctx.lineWidth = 3; ctx.stroke()
                                                ctx.globalAlpha = 0.55 * lift * alpha
                                                ctx.lineWidth = 1; ctx.stroke()
                                            }

                                            // One shared gutter and cover edge, independent of the leaf.
                                            // The landed spread and the next turn's start are the same image.
                                            ctx.strokeStyle = ink
                                            ctx.globalAlpha = 0.8
                                            ctx.lineWidth = 1
                                            ctx.beginPath()
                                            ctx.moveTo(12, 5); ctx.lineTo(12, 15)
                                            ctx.moveTo(1.5, 15); ctx.lineTo(7, 14.5)
                                            ctx.lineTo(12, 16); ctx.lineTo(17, 14.5)
                                            ctx.lineTo(22.5, 15)
                                            ctx.stroke()
                                        }
                                    }
                                }
                            }
                            // ── the SCREEN PANE — agent rows only ─────────────
                            // The DELIBERATE carve-out from "terminals should
                            // look like the conductor" (the owner's own): the
                            // agent-specific info core keeps this temple's OWN
                            // treatment. The Conductor gives every agent a full
                            // dedicated card; this roster packs the same agent
                            // into one row among many — the density is
                            // legitimately different, so the little aegean
                            // screen stays: a hairline bezel over a faint wash,
                            // line one the PROMPT ($ sigil + the live tool/
                            // command, WRAPPING to three lines now — a long
                            // command reads, never just truncates — with the
                            // model in a fixed right cell, a middle-elided view
                            // of the REAL id, pantheon §4), then THREE reserved
                            // lines of the agent's latest words, serif-italic
                            // quoted. The say lane is FIXED; the pane grows
                            // only as the command wraps. Bare ttys skip the
                            // pane entirely (a Column skips invisible children).
                            Rectangle {
                                id: screenPane
                                visible: row.agentRow
                                width: parent.width
                                height: paneCmd.height + 48   // 4 top + cmd + 4 gap + 36 say + 4 base
                                radius: 0
                                color: gadget.withA(gadget.sig, 0.06)
                                border.width: 1
                                border.color: row.emph ? gadget.withA(livery.paletteHot, 0.45)
                                                       : gadget.withA(gadget.sig, 0.30)

                                Text {                     // the prompt sigil
                                    id: promptSigil
                                    x: 6; y: 4
                                    text: "$"
                                    font.family: gadget.faceMono; font.pixelSize: 10
                                    color: gadget.withA(gadget.sig, 0.95)
                                }
                                Text {                     // the live tool / command — wraps
                                                            // up to three lines (commands
                                                            // carry spaces, so plain Wrap
                                                            // breaks at them; the third line
                                                            // still elides past the cap).
                                                            // The pane's height rides this.
                                    id: paneCmd
                                    anchors.left: promptSigil.right; anchors.leftMargin: 5
                                    anchors.right: modelCell.visible ? modelCell.left : parent.right
                                    anchors.rightMargin: 6
                                    anchors.top: parent.top; anchors.topMargin: 3
                                    wrapMode: Text.Wrap
                                    maximumLineCount: 3
                                    elide: Text.ElideRight
                                    text: row.toolText.length > 0
                                          ? row.toolText
                                          : (modelData.agent || "agent")
                                    font.family: gadget.faceMono; font.pixelSize: 10
                                    color: gadget.withA(livery.paletteFg,
                                                        row.toolText.length > 0 ? 0.85 : 0.45)
                                }
                                Text {                     // the MODEL — a fixed right cell
                                                            // on the prompt line, baseline-
                                                            // matched to the $ sigil; middle-
                                                            // elided view of the REAL model
                                                            // id (pantheon §4).
                                    id: modelCell
                                    visible: row.agentRow && (modelData.model || "").length > 0
                                    anchors.right: parent.right; anchors.rightMargin: 6
                                    anchors.baseline: promptSigil.baseline
                                    width: 100
                                    horizontalAlignment: Text.AlignRight
                                    elide: Text.ElideMiddle
                                    text: modelData.model || ""
                                    font.family: gadget.faceMono; font.pixelSize: 9
                                    color: gadget.withA(livery.paletteFg, 0.55)   // Conductor's model dim
                                }
                                Text {                     // the WORDS — 3 reserved lines
                                    anchors.left: parent.left; anchors.leftMargin: 6
                                    anchors.right: parent.right; anchors.rightMargin: 6
                                    anchors.top: paneCmd.bottom; anchors.topMargin: 4
                                    height: 36
                                    text: row.sayFlat.length > 0 ? "“" + row.sayFlat + "”" : "…"
                                    wrapMode: Text.WordWrap
                                    maximumLineCount: 3
                                    elide: Text.ElideRight
                                    lineHeight: 12
                                    lineHeightMode: Text.FixedHeight
                                    font.family: gadget.faceSerif; font.italic: true
                                    font.pixelSize: 10
                                    color: gadget.withA(livery.paletteFg,
                                                        row.sayFlat.length > 0 ? 0.62 : 0.30)
                                }
                            }

                            // ── VITALS — elapsed · the ctx meter ──────────────
                            // The same carve-out: this temple's own compact
                            // treatment, not the Conductor's ctx/up pulse line.
                            // Elapsed tallied plain in aegean; agent rows add
                            // the CONTEXT-WINDOW METER — bar + percent + count
                            // in the dock's [▓░] ASCII-gauge grammar (AoideBar
                            // battBar / MetersGadget barFill), zero footprint
                            // until an assistant turn publishes usage.
                            Item {
                                width: parent.width
                                height: 13

                                Row {
                                    anchors.left: parent.left; anchors.leftMargin: 21
                                    anchors.right: parent.right
                                    anchors.verticalCenter: parent.verticalCenter
                                    spacing: 8
                                    Text {                     // elapsed, tallied in aegean
                                        id: elapsedText
                                        text: gadget.elapsed(modelData.startedAt)
                                        font.family: gadget.faceMono; font.pixelSize: 10
                                        color: row.emph ? livery.paletteHot : gadget.sig
                                    }
                                    Text {                     // the [▓░] meter — agent rows only
                                        id: ctxTag
                                        anchors.baseline: elapsedText.baseline
                                        visible: row.hasCtx
                                        text: livery.ctxBar(row.ctxPct, 6) + " "
                                              + Math.round(row.ctxPct) + "% · "
                                              + livery.ctxCompact(modelData.contextTokens)
                                        font.family: gadget.faceMono; font.pixelSize: 10
                                        // song accent → paletteUrgent past ~85%, the
                                        // shared urgency threshold.
                                        color: livery.ctxColor(row.ctxPct, gadget.sig)
                                    }
                                }
                            }
                            // ── 6 · GROUND — cwd ── the troupe, one line ──────
                            // The Conductor's ground: cwd left, the animated
                            // mood face right in its reserved clipped box (116px
                            // agent / 90px tty — main/sub scale). The cwd keeps
                            // this roster's one sanctioned growth: a REAL dir
                            // WRAPS onto new lines when it overflows (a yazi
                            // session's live dir is long and space-free — one
                            // elided line would hide which dir it is on), while
                            // the TITLE fallback (a bare tty's window title,
                            // often the running command) stays a single middle-
                            // elided line so it can't balloon the plaque.
                            Item {
                                width: parent.width
                                height: Math.max(row.agentRow ? 17 : 15, cwdText.implicitHeight)

                                Item {                     // reserved, clipped troupe box
                                    id: faceBox
                                    anchors.right: parent.right
                                    anchors.verticalCenter: parent.verticalCenter
                                    width: row.agentRow ? 116 : 90
                                    height: row.agentRow ? 17 : 15
                                    clip: true
                                    Text {
                                        id: kao            // the mood face
                                        anchors.right: parent.right
                                        anchors.verticalCenter: parent.verticalCenter
                                        // WORKING animates; every resting state
                                        // holds one pose. Terminals have no
                                        // subagent concept, so every row wears
                                        // the general `working` pool — HASHED
                                        // from the row's stable key (pickFor/
                                        // phaseFor), never drawn at random: the
                                        // roster model gets reassigned on
                                        // ordinary heartbeats, far more often
                                        // than it actually changes, so a random
                                        // pick would visibly reshuffle. `rowKey`
                                        // is the same stable identity used for
                                        // hover-across-rebuild above.
                                        readonly property string moodKey: gadget.rowKey(modelData)
                                        property int setIdx: gadget.faces.pickFor(moodKey, gadget.faces.working)
                                        property int frame: gadget.faces.phaseFor(moodKey, frames.length)
                                        readonly property var frames: gadget.faces.workingFrames(setIdx)
                                        text: row.working ? frames[frame % frames.length]
                                                          : gadget.kaomojiFor(modelData.state)
                                        // Explicit family: the general pool
                                        // carries the parcel glyph, and that
                                        // icon lives in the Nerd Font's private-
                                        // use range — Qt's CJK/kana fallback has
                                        // no claim on it.
                                        font.family: gadget.faceMono
                                        font.pixelSize: row.agentRow ? 10 : 9
                                        color: row.emph ? livery.paletteHot
                                                        : gadget.withA(row.accent, 0.85)
                                        Timer {
                                            running: row.working
                                            repeat: true; interval: 300
                                            onTriggered: kao.frame = (kao.frame + 1) % kao.frames.length
                                        }
                                    }
                                }
                                Text {                     // cwd — left-anchored; a real
                                                            // dir wraps, the title fallback
                                                            // middle-elides (see above)
                                    id: cwdText
                                    anchors.left: parent.left; anchors.leftMargin: 21
                                    anchors.right: faceBox.left; anchors.rightMargin: 8
                                    anchors.verticalCenter: parent.verticalCenter
                                    horizontalAlignment: Text.AlignLeft
                                    wrapMode: row.cwdReal ? Text.WrapAnywhere : Text.NoWrap
                                    elide: row.cwdReal ? Text.ElideRight : Text.ElideMiddle
                                    text: gadget.shortCwd(modelData.cwd) || (modelData.title || "")
                                    font.family: gadget.faceMono; font.pixelSize: 10
                                    color: gadget.withA(gadget.sig, 0.85)   // the Conductor's cwd dim (its holoBlue IS this sig)
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
                                // (the one narrow outbound socket command).
                                if (modelData.sessionId && gadget.bridge && gadget.bridge.focusSession)
                                    gadget.bridge.focusSession(modelData.sessionId);
                            }
                        }

                            }
                        }
                    }
                }

                // slim aegean scrollbar in the roster gutter — draggable
                // (ScrollRail). This rail sits OVER the flick, so the grab lane
                // is held to 3px a side: it reaches only into the playbill's own
                // 6px gutter and leaves the row plaques clickable.
                ScrollRail {
                    flick: flick
                    railW: 3
                    minThumb: 20
                    grabPad: 3
                    trackColor: gadget.withA(livery.paletteFg, 0.10)
                    thumbColor: gadget.withA(gadget.sig, 0.75)
                    anchors.top: flick.top; anchors.bottom: flick.bottom
                    anchors.right: parent.right; anchors.rightMargin: 3
                }
            }
        }
    }
}
