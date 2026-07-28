// TerminalManagerGadget.qml — the Terminal-Commander roster as a dock gadget.
//
// One row per live agent session, read from song/stage/sessions.json and
// song/stage/hooks.json — BOTH watched via the DrachmaState FileView pattern
// (atomic write-temp-then-rename → onTextChanged → recompute). The latest hook
// phase (by updatedAt; ties → later record wins) overrides the roster state,
// mirroring `aoide graph`'s merged_sessions() EXACTLY (see pkgs/aoide/src/
// graph.rs) so the dock and the CLI agree on live state.
//
// Per row: a ⠿ drag handle, agent, colored state badge, short cwd, elapsed since
// startedAt.
//   state color (from notes, zero hardcoded hex):
//     running / active                 → paletteAccent
//     awaiting / Notification-ish      → paletteUrgent
//     done                             → paletteFg dimmed (0.5)
//     other (idle, …)                  → paletteFg
// Row click → bridge.focusSession(windowAddress) (the shellbridge session-jump
// gate; QML never shells out).
//
// ── Reorder + DAG trace (v2) ────────────────────────────────────────────────
// The merged roster feeds a LOCAL ListModel (`roster`) rather than the Repeater
// directly: a ListModel's move() preserves delegate instances, so a row can be
// dragged by its ⠿ handle and re-slotted live (an array reassign would destroy
// the delegate mid-drag and drop the grab). syncRoster() keeps `roster` in step
// with the file-derived `sessions` — update-in-place / append-new / drop-gone —
// so a hot-reload never clobbers a manual reorder. The order is purely visual
// and session-scoped (no persistence, no CLI surface — we invent no IPC).
//
// Hovering a row publishes its sessionId to shared.tracedSessionId; DagGraphGadget
// highlights the node whose id is `session:<that id>`. The trace is the shared
// hover state ONLY — both gadgets already read the same stage.
//
// Prune affordance: each row shows a small [x] ASCII affordance meant to prune
// `done` sessions. DECISION — the shellbridge SOCKET protocol (ShellBridge.qml)
// currently exposes only `focuswindow`; the `graph prune` verb exists as a CLI
// command (aoide graph prune) but there is NO socket/bridge verb for it. Per the
// task, we DO NOT invent new IPC: the [x] is rendered DISABLED (dimmed, no
// handler) on done rows until ShellBridge grows a prune verb.

import QtQuick
import Quickshell
import Quickshell.Io

Item {
    id: root

    // ── Note + bridge dependencies (injected by the dock) ──────────────────
    required property var notes
    required property var bridge
    // Shared session state — the DAG trace link (optional; null when hosted
    // somewhere that doesn't wire it). Row hover writes shared.tracedSessionId.
    property var shared: null

    // ── Parsed stage documents (v0 shapes; see CONTRACTS.md §4) ────────────
    property var sessionsDoc: ({ "schemaVersion": "0", "sessions": [] })
    property var hooksDoc: ({ "schemaVersion": "0", "hooks": [] })

    // ── Live "now" tick for elapsed rendering ──────────────────────────────
    property double nowMs: Date.now()

    // ── Stage paths ─────────────────────────────────────────────────────────
    readonly property string sessionsPath:
        Quickshell.env("HOME") + "/Aoide/song/stage/sessions.json"
    readonly property string hooksPath:
        Quickshell.env("HOME") + "/Aoide/song/stage/hooks.json"

    // ── Merged roster ───────────────────────────────────────────────────────
    // Mirrors graph.rs::merged_sessions: overlay latest hook phase over roster
    // state, then sort by (startedAt, sessionId). Machine-checked in node.
    readonly property var sessions: mergeSessions(root.sessionsDoc, root.hooksDoc)
    onSessionsChanged: syncRoster()

    function mergeSessions(sdoc, hdoc) {
        var sessions = (sdoc && sdoc.sessions) ? sdoc.sessions : []
        var hooks = (hdoc && hdoc.hooks) ? hdoc.hooks : []

        // latest[sessionId] = { updatedAt, phase } — keep the max updatedAt;
        // ties resolve to the later record in file order (matches the Rust:
        // strictly-less keeps the existing, so equal/greater overwrites).
        var latest = ({})
        for (var i = 0; i < hooks.length; i++) {
            var h = hooks[i]
            if (!h || h.sessionId === undefined)
                continue
            var cur = latest[h.sessionId]
            var at = h.updatedAt || ""
            if (cur && at < cur.updatedAt)
                continue
            latest[h.sessionId] = { "updatedAt": at, "phase": h.phase || "" }
        }

        // Copy roster, overlay non-empty phase.
        var merged = []
        for (var j = 0; j < sessions.length; j++) {
            var s = sessions[j]
            var copy = ({})
            for (var k in s) copy[k] = s[k]
            var lp = latest[copy.sessionId]
            if (lp && lp.phase && lp.phase.length > 0)
                copy.state = lp.phase
            merged.push(copy)
        }

        // Deterministic order: (startedAt, sessionId), string compare — same as
        // the Rust tuple sort.
        merged.sort(function (a, b) {
            var aa = (a.startedAt || "") + " " + (a.sessionId || "")
            var bb = (b.startedAt || "") + " " + (b.sessionId || "")
            return aa < bb ? -1 : (aa > bb ? 1 : 0)
        })
        return merged
    }

    // ── Local reorderable model ──────────────────────────────────────────────
    // syncRoster() reconciles `roster` with `sessions`: drop rows whose session
    // vanished, update surviving rows in place (preserving any manual order),
    // append newcomers. Called on every recompute of `sessions`.
    property ListModel roster: ListModel {}

    function rowObj(s) {
        return {
            "sessionId": s.sessionId || "",
            "agent": s.agent || "",
            "state": s.state || "",
            "cwd": s.cwd || "",
            "startedAt": s.startedAt || "",
            "windowAddress": s.windowAddress || "",
            // Hyprland workspace id (stamped by the shellbridge window-event
            // listener); -1 = unknown/unresolved → hovering highlights nothing.
            "workspace": (s.workspace !== undefined && s.workspace !== null) ? s.workspace : -1
        }
    }
    function syncRoster() {
        var list = root.sessions
        var present = ({})
        for (var i = 0; i < list.length; i++)
            present[list[i].sessionId] = list[i]

        // Drop rows whose session is gone (iterate backwards).
        for (var r = roster.count - 1; r >= 0; r--)
            if (!present[roster.get(r).sessionId])
                roster.remove(r)

        // Update survivors in place; remember which ids we've placed.
        var seen = ({})
        for (var k = 0; k < roster.count; k++) {
            var id = roster.get(k).sessionId
            if (present[id]) {
                roster.set(k, rowObj(present[id]))
                seen[id] = true
            }
        }

        // Append newcomers at the tail (deterministic `sessions` order).
        for (var j = 0; j < list.length; j++)
            if (!seen[list[j].sessionId])
                roster.append(rowObj(list[j]))
    }

    // ── Neon-dominance tuning (round 2) — the traced row is THE hot element ─
    // across BOTH gadgets, so its highlight matches the DAG's traced-node blaze:
    // 2px border, a soft two-ring halo (the depth-stack trick as a glow).
    property real haloOpacity1: 0.35 // inner ring (row grown +2)
    property real haloOpacity2: 0.15 // mid ring (row grown +4)
    property real haloOpacity3: 0.08 // outer bloom ring (row grown +6)

    // ── State → glyph — baton's theme.rs vocabulary (grammar's state tier) ──
    // ♪ working · 𝄐 awaiting · 𝄽 idle · 𝄂 done · · unknown. Kept in lockstep
    // with BatonGadget/DagGraphGadget so every surface reads the same score.
    // Replaces the old [running]/[awaiting]/[done] word-badges (grammar §3).
    function stateGlyph(state) {
        var l = ("" + (state || "")).toLowerCase()
        if (l === "done" || l === "stop")
            return "𝄂"
        if (l.indexOf("await") !== -1 || l.indexOf("block") !== -1 || l === "notification")
            return "𝄐"
        if (l.indexOf("running") !== -1 || l.indexOf("pretooluse") !== -1
            || l.indexOf("posttooluse") !== -1 || l.indexOf("active") !== -1
            || l.indexOf("tool") !== -1)
            return "♪"
        if (l.indexOf("idle") !== -1)
            return "𝄽"
        return "·"
    }

    // ── State → colour map (notes only) ─────────────────────────────────────
    function stateColor(state) {
        var s = ("" + (state || "")).toLowerCase()
        if (s === "running" || s === "active")
            return notes.paletteAccent
        if (s === "done")
            return notes.paletteFg
        // Blocked — a permission prompt sits mid-turn, a human is summoned: the
        // Pantheon urgent role (glitchPink, base08), hotter than notif/await.
        if (s.indexOf("block") !== -1)
            return notes.glitchPink
        if (s.indexOf("notif") !== -1 || s.indexOf("await") !== -1)
            return notes.paletteUrgent
        return notes.paletteFg
    }
    function isDoneState(state) {
        return ("" + (state || "")).toLowerCase() === "done"
    }
    function isBlocked(state) {
        return ("" + (state || "")).toLowerCase().indexOf("block") !== -1
    }

    // ── Short cwd (last two path segments) — mirrors GraphRow.shortCwd ──────
    function shortCwd(cwd) {
        if (!cwd)
            return ""
        var parts = ("" + cwd).split("/").filter(function (p) { return p.length > 0 })
        if (parts.length <= 2)
            return cwd
        return "…/" + parts.slice(-2).join("/")
    }

    // ── Elapsed formatting: startedAt (ISO-8601) → "3m", "2h04m", "1d3h" ───
    function elapsed(startedAt, nowMs) {
        if (!startedAt)
            return ""
        var t = Date.parse(startedAt)
        if (isNaN(t))
            return ""
        var sec = Math.floor((nowMs - t) / 1000)
        if (sec < 0) sec = 0
        if (sec < 60)
            return sec + "s"
        var min = Math.floor(sec / 60)
        if (min < 60)
            return min + "m"
        var hr = Math.floor(min / 60)
        var remMin = min % 60
        if (hr < 24)
            return hr + "h" + (remMin < 10 ? "0" : "") + remMin + "m"
        var day = Math.floor(hr / 24)
        var remHr = hr % 24
        return day + "d" + remHr + "h"
    }

    implicitHeight: content.implicitHeight

    Component.onCompleted: syncRoster()

    Timer {
        interval: 1000
        repeat: true
        running: true
        onTriggered: root.nowMs = Date.now()
    }

    Column {
        id: content
        width: parent ? parent.width : implicitWidth
        spacing: 2

        readonly property int rowH: 20

        // ── Empty state ──────────────────────────────────────────────────
        Text {
            visible: root.roster.count === 0
            width: parent.width
            text: "no sessions"
            color: notes.paletteFg
            opacity: 0.6
            font.family: "monospace"
            font.pixelSize: 12
        }

        // ── Session rows ─────────────────────────────────────────────────
        Repeater {
            model: root.roster
            delegate: Item {
                id: rowItem
                width: content.width
                implicitHeight: content.rowH

                required property int index
                required property string sessionId
                required property string agent
                required property string state
                required property string cwd
                required property string startedAt
                required property string windowAddress
                required property int workspace

                readonly property bool done: root.isDoneState(state)
                readonly property bool blocked: root.isBlocked(state)
                readonly property bool traced:
                    root.shared && root.shared.tracedSessionId === sessionId
                                && sessionId.length > 0

                // Blocked rows pulse — the urgent-pulse idiom from WorkspaceRow
                // (blink_red homage): a 600ms InOutQuad breath to 0.35 and back,
                // forever, so the summoned human's eye lands on the row.
                SequentialAnimation on opacity {
                    running: rowItem.blocked
                    loops: Animation.Infinite
                    NumberAnimation { to: 0.35; duration: 600; easing.type: Easing.InOutQuad }
                    NumberAnimation { to: 1.0;  duration: 600; easing.type: Easing.InOutQuad }
                }

                // ── Neon halo (traced row only) — matches DAG's traced node ──
                // THREE transparent rings, the row grown +6/+4/+2, in the HOT
                // neon (green) so the traced row glows as a bloom. Declared first
                // → behind the row fill. Border-only, no MouseArea (input-inert).
                // The trace link reads as THE one green element across both
                // gadgets; the rose accent stays chrome.
                Rectangle {
                    visible: rowItem.traced
                    anchors.fill: parent
                    anchors.margins: -6
                    radius: 0
                    color: "transparent"
                    border.color: notes.paletteHot
                    border.width: 1
                    opacity: root.haloOpacity3
                }
                Rectangle {
                    visible: rowItem.traced
                    anchors.fill: parent
                    anchors.margins: -4
                    radius: 0
                    color: "transparent"
                    border.color: notes.paletteHot
                    border.width: 1
                    opacity: root.haloOpacity2
                }
                Rectangle {
                    visible: rowItem.traced
                    anchors.fill: parent
                    anchors.margins: -2
                    radius: 0
                    color: "transparent"
                    border.color: notes.paletteHot
                    border.width: 1
                    opacity: root.haloOpacity1
                }
                Rectangle {
                    anchors.fill: parent
                    radius: 0
                    color: (rowItem.traced || rowHover.hovered) ? notes.barBg : "transparent"
                    // Traced row blazes HOT (green); a plain hover shows no border.
                    border.color: rowItem.traced ? notes.paletteHot : notes.paletteAccent
                    border.width: rowItem.traced ? 2 : 0
                }

                Row {
                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 6

                    // Drag handle — grip glyph; drag to re-slot the row.
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        text: "⠿"
                        color: notes.paletteAccent
                        opacity: rowItem.done ? 0.4 : 0.7
                        font.family: "monospace"
                        font.pixelSize: 12
                    }
                    // Agent
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        text: rowItem.agent || "?"
                        color: root.stateColor(rowItem.state)
                        opacity: rowItem.done ? 0.5 : 1.0
                        font.family: "monospace"
                        font.pixelSize: 12
                        font.bold: rowItem.traced
                    }
                    // State GLYPH — baton vocabulary (grammar §2 state tier),
                    // replacing the old [running]/[awaiting] word-badge.
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        text: root.stateGlyph(rowItem.state)
                        color: root.stateColor(rowItem.state)
                        opacity: rowItem.done ? 0.5 : 1.0
                        font.family: "monospace"
                        font.pixelSize: 12
                    }
                    // Dim lowercase callout line — cwd · elapsed. The refs'
                    // recessive label pattern; the glyph carries the live state.
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        text: root.shortCwd(rowItem.cwd).toLowerCase()
                        color: notes.paletteFg
                        opacity: rowItem.done ? 0.3 : 0.45
                        font.family: "monospace"
                        font.pixelSize: 11
                        elide: Text.ElideRight
                    }
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        text: root.elapsed(rowItem.startedAt, root.nowMs)
                        color: notes.paletteFg
                        opacity: rowItem.done ? 0.3 : 0.4
                        font.family: "monospace"
                        font.pixelSize: 11
                    }
                }

                // ── Prune affordance [x] — DISABLED (see file header) ─────
                Text {
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    visible: rowItem.done
                    text: "[x]"
                    color: notes.paletteFg
                    opacity: 0.25
                    font.family: "monospace"
                    font.pixelSize: 11
                }

                // ── Row hover → DAG trace + workspace-preview publish ─────
                // Publishes TWO shared-state links on hover (coexists with the
                // click-to-jump MouseArea below — a HoverHandler never steals the
                // click, and the dock's panel-wide hover union keeps the drawer
                // open): (1) tracedSessionId → DagGraphGadget highlights the node;
                // (2) hoveredWorkspace → the bar's WorkspaceRow preview-highlights
                // the workspace this terminal lives on. Both clear on exit, guarded
                // by the (unique) sessionId so crossing between rows never wipes the
                // newly-hovered row's state. Unknown workspace (-1) highlights
                // nothing — no error. Sentinel clear: -1.
                HoverHandler {
                    id: rowHover
                    onHoveredChanged: {
                        if (!root.shared)
                            return
                        if (hovered) {
                            root.shared.tracedSessionId = rowItem.sessionId
                            root.shared.hoveredWorkspace = rowItem.workspace
                        } else if (root.shared.tracedSessionId === rowItem.sessionId) {
                            root.shared.tracedSessionId = ""
                            root.shared.hoveredWorkspace = -1
                        }
                    }
                }
                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: {
                        if (rowItem.windowAddress)
                            root.bridge.focusSession(rowItem.windowAddress)
                    }
                }

                // ── Drag-to-reorder — over the ⠿ handle at the left edge ──
                // Declared LAST so it stacks above the row-fill click MouseArea.
                // Column fixes each child's y, so we don't float the row; instead
                // we move() it in the model as the cursor crosses row pitches —
                // the row jumps to its new slot and Column re-lays out.
                MouseArea {
                    id: dragHandle
                    anchors.left: parent.left
                    anchors.top: parent.top
                    anchors.bottom: parent.bottom
                    width: 18
                    cursorShape: Qt.SizeVerCursor
                    onPositionChanged: function (mouse) {
                        if (!pressed)
                            return
                        var p = mapToItem(content, mouse.x, mouse.y)
                        var pitch = content.rowH + content.spacing
                        var target = Math.floor(p.y / pitch)
                        if (target < 0) target = 0
                        if (target > root.roster.count - 1) target = root.roster.count - 1
                        if (target !== rowItem.index)
                            root.roster.move(rowItem.index, target, 1)
                    }
                }
            }
        }
    }

    // ── sessions.json watcher (DrachmaState pattern) ──────────────────────────
    FileView {
        id: sessionsFile
        path: root.sessionsPath
        watchChanges: true
        onFileChanged: sessionsFile.reload()
        onTextChanged: {
            try {
                root.sessionsDoc = JSON.parse(sessionsFile.text())
            } catch (e) {
                console.warn("[aoide/terminals] parse sessions.json:", e)
            }
        }
        Component.onCompleted: sessionsFile.reload()
    }

    // ── hooks.json watcher (DrachmaState pattern) ─────────────────────────────
    FileView {
        id: hooksFile
        path: root.hooksPath
        watchChanges: true
        onFileChanged: hooksFile.reload()
        onTextChanged: {
            try {
                root.hooksDoc = JSON.parse(hooksFile.text())
            } catch (e) {
                console.warn("[aoide/terminals] parse hooks.json:", e)
            }
        }
        Component.onCompleted: hooksFile.reload()
    }
}
