// DrachmaState.qml — shared note state, hot-reloaded from stage/drachma.json.
//
// Singleton: every surface widget binds to properties here. When drachma.json
// is atomically replaced (write-temp-then-rename per CONTRACTS.md §4), the
// FileView fires a change signal and all bindings update in one pass — the
// full arrangement hot-reloads without a QML restart.
//
// Note schema v0 (CONTRACTS.md §1): palette + bar.* / notif.* / window.*
// All values are concrete hex strings (fallbacks already applied by the note
// emitter — Quickshell never sees null).

import QtQuick
import Quickshell
import Quickshell.Io

QtObject {
    id: root

    // ── Note file path ─────────────────────────────────────────────────────
    // Stage path: ~/Aoide/song/stage/drachma.json (gitignored runtime; the nix
    // build never depends on this path — checks.no-song-read enforces that).
    readonly property string notePath:
        Quickshell.env("HOME") + "/Aoide/song/stage/drachma.json"

    // ── Parsed note object ─────────────────────────────────────────────────
    property var raw: ({
        "schemaVersion": "0",
        "palette": { "bg": "#f4ecdc", "fg": "#423420", "accent": "#4f74a0", "urgent": "#b0475f", "hot": "#6f8a4f" },
        "base16": { "base08": "#b0475f", "base0C": "#40897a", "base0D": "#4f74a0", "base0E": "#8f5578" },
        "bar":    { "bg": "#f4ecdc", "fg": "#423420", "accent": "#4f74a0" },
        "notif":  { "bg": "#f4ecdc", "fg": "#423420", "urgent": "#b0475f" },
        "window": { "border": "#40897a", "borderInactive": "#eaddc6" }
    })

    // ── Song name ───────────────────────────────────────────────────────────
    // Additive optional field (`aoide rice preview <name>` injects `song` into
    // the staged notes — dispatch.rs handle_rice_preview) — drives per-song
    // flavor-widget resolution (the staging engine: StagingEngine.qml /
    // WidgetSlot.qml). Absent (a
    // notes file staged some other way) means "no song identity" — resolvers
    // treat "" as "nothing authored", never a crash.
    readonly property string songName: (raw.song) ? raw.song : ""

    // ── Palette shortcuts ──────────────────────────────────────────────────
    readonly property color paletteBg:     raw.palette ? raw.palette.bg     : "#f4ecdc"
    readonly property color paletteFg:     raw.palette ? raw.palette.fg     : "#423420"
    readonly property color paletteAccent: raw.palette ? raw.palette.accent : "#4f74a0"
    readonly property color paletteUrgent: raw.palette ? raw.palette.urgent : "#b0475f"
    // Hot/trace highlight — the one-neon element (optic-nerve green). Optional
    // in the v0 note schema: falls back to the accent when palette.hot is
    // absent, so a note file without it renders exactly as before.
    readonly property color paletteHot:
        (raw.palette && raw.palette.hot) ? raw.palette.hot : paletteAccent

    // ── Base16 scheme → semantic Pantheon accents ──────────────────────────
    // The optional top-level base16 block (drachma's base16 tier) carries the
    // full sixteen-slot terminal scheme. The Pantheon wireframe field draws its
    // multicolor accents from it under semantic names — the cool wireframe cyan,
    // hologram periwinkle, violet brain-glow and glitch pink of the reference
    // stills. When the block is ABSENT every accent falls back to paletteAccent,
    // so a note file without base16 renders exactly as the round-3 rose field.
    readonly property color wireCyan:   // base0C — wireframe outlines + leaders
        (raw.base16 && raw.base16.base0C) ? raw.base16.base0C : paletteAccent
    readonly property color holoBlue:   // base0D — depth-stack back copies
        (raw.base16 && raw.base16.base0D) ? raw.base16.base0D : paletteAccent
    readonly property color violet:     // base0E — DAG project volumes
        (raw.base16 && raw.base16.base0E) ? raw.base16.base0E : paletteAccent
    readonly property color glitchPink: // base08 — reserved glitch/alt accent
        (raw.base16 && raw.base16.base08) ? raw.base16.base08 : paletteAccent
    readonly property color base09:     // base09 — unnamed elsewhere; bar note glyphs
        (raw.base16 && raw.base16.base09) ? raw.base16.base09 : paletteAccent
    readonly property color base0F:     // base0F — rust; unnamed elsewhere; bar note glyphs
        (raw.base16 && raw.base16.base0F) ? raw.base16.base0F : paletteAccent

    // ── Accent spread — the 8-hue base16 accent cycle, in base08→base0F order.
    // Consumers that want a DISTINCT colour per small integer id (e.g. the bar's
    // per-workspace note glyphs) call noteColor(id) rather than reaching into
    // base16 directly — keeps the cycle order defined in one place.
    readonly property var accentSpread: [
        base0F,        // base0F rust (reversed-order lead: past-violet brown, now first)
        violet,        // base0E murex
        holoBlue,      // base0D aegean
        wireCyan,      // base0C teal
        paletteHot,    // base0B laurel
        paletteAccent, // base0A gold
        base09,        // base09
        glitchPink     // base08 terracotta
    ]
    function noteColor(id) {
        var n = accentSpread.length
        var i = ((id - 1) % n + n) % n   // 1-based id, safe for id <= 0 too
        return accentSpread[i]
    }

    // ── Bar component shortcuts ────────────────────────────────────────────
    readonly property color barBg:     raw.bar ? raw.bar.bg     : paletteBg
    readonly property color barFg:     raw.bar ? raw.bar.fg     : paletteFg
    readonly property color barAccent: raw.bar ? raw.bar.accent : paletteAccent

    // ── Notif component shortcuts ──────────────────────────────────────────
    readonly property color notifBg:     raw.notif ? raw.notif.bg     : paletteBg
    readonly property color notifFg:     raw.notif ? raw.notif.fg     : paletteFg
    readonly property color notifUrgent: raw.notif ? raw.notif.urgent : paletteUrgent

    // ── Window component shortcuts ─────────────────────────────────────────
    readonly property color windowBorder:         raw.window ? raw.window.border         : paletteAccent
    readonly property color windowBorderInactive: raw.window ? raw.window.borderInactive : paletteBg

    // ── Context-window meter helpers (shared by ConductorGadget/TerminalsGadget) ──
    // A session row's `contextTokens` (SessionRecord.context_tokens, plumbed
    // through graph.json / sessions.json — CONTRACTS.md §4) is the raw
    // input-side token count of the session's LAST request. Turning that into
    // a meter takes two client-side steps, both pure and shared here so
    // neither gadget computes it its own (possibly diverging) way.

    // Known/likely long-context model-id substrings → their ceiling. Today's
    // transcripts never actually spell one of these out (Claude Code logs the
    // bare id, e.g. `claude-opus-4-8`, even when the session runs the
    // marketed "[1m]" 1M-context tier) — this map stays a defensive
    // placeholder for the day an id DOES carry an explicit marker. The real
    // signal today is `ctx1mBase` below, read straight from the user's
    // ~/.claude/settings.json; the >200k-tokens heuristic in `ctxCeiling`
    // remains only as the last-resort backstop when neither the map nor the
    // config knows better.
    readonly property var ctxCeilingMap: ({
        "[1m]": 1000000
    })
    // ── 1M-context tier, read from ~/.claude/settings.json ──────────────────
    // Claude Code's own settings carry a top-level `"model"` string that
    // spells the marketed context tier as a bracket suffix, e.g.
    // `"opus[1m]"` — the transcript's model id itself never does. Reading
    // this file is what lets `ctxCeiling` recognize a 1M-context session from
    // its very first turn, rather than only after 200k+ tokens have already
    // accumulated.
    readonly property string settingsPath:
        Quickshell.env("HOME") + "/.claude/settings.json"
    // The base model alias running the 1M tier per settings.json (e.g.
    // "opus", from "opus[1m]"), or "" when the file is absent, unparseable,
    // or the configured model carries no `[1m]` marker. `ctxCeiling` treats
    // "" as "no config signal" and falls back to its map + backstop.
    property string ctx1mBase: ""
    property FileView settingsFile: FileView {
        id: settingsFile
        path: root.settingsPath
        watchChanges: true
        blockLoading: false
        printErrors: false
        onTextChanged: {
            try {
                var parsed = JSON.parse(settingsFile.text())
                var m = (parsed && parsed.model) ? String(parsed.model) : ""
                if (/\[1m\]/i.test(m)) {
                    var lower = m.toLowerCase()
                    var bracketAt = lower.indexOf("[")
                    root.ctx1mBase = (bracketAt >= 0 ? lower.substring(0, bracketAt) : lower).trim()
                } else {
                    root.ctx1mBase = ""
                }
            } catch (e) {
                root.ctx1mBase = "" // absent/mid-write/garbage → no config signal
            }
        }
        onFileChanged: settingsFile.reload()
        Component.onCompleted: settingsFile.reload()
    }
    // The percentage ceiling for a session's model id, given its OWN token
    // count. Starts from `ctxCeilingMap` (default 200000 when no substring
    // matches), then folds in the config-derived 1M tier (`ctx1mBase`, from
    // ~/.claude/settings.json), then applies the heuristic backstop: a token
    // count that has already blown past 200k could only belong to a
    // 1M-context session (a 200k-context request is hard-capped there), so
    // the ceiling is raised to 1000000 rather than letting the bar pin at a
    // false 100%. The final ceiling is the max of all three signals.
    function ctxCeiling(modelId, tokens) {
        var id = (modelId || "").toLowerCase()
        var mapped = 200000
        for (var key in ctxCeilingMap) {
            if (id.indexOf(key) >= 0) { mapped = ctxCeilingMap[key]; break }
        }
        if (ctx1mBase !== "" && id.indexOf(ctx1mBase) >= 0) { mapped = Math.max(mapped, 1000000) }
        return Math.max(mapped, (tokens || 0) > 200000 ? 1000000 : mapped)
    }
    // contextTokens / ceiling(model, contextTokens), clamped 0-100.
    function ctxPercent(modelId, tokens) {
        var t = tokens || 0
        if (t <= 0) return 0
        var pct = t / ctxCeiling(modelId, t) * 100
        return pct < 0 ? 0 : (pct > 100 ? 100 : pct)
    }
    // Compact token-count label: <1000 → raw, ≥1000 → `Nk`, ≥1e6 → `N.NM`.
    function ctxCompact(n) {
        var v = n || 0
        if (v < 1000) return String(v)
        if (v < 1000000) return Math.round(v / 1000) + "k"
        return (v / 1000000).toFixed(1) + "M"
    }
    // The `[▓▓░░░░]`-style bar (the dock's existing ASCII-gauge grammar — see
    // AoideBar.qml `battBar` / MetersGadget.qml `barFill`), sized to `cells`
    // characters. One string, one Text, one colour: the ▓/░ glyphs themselves
    // (heavy vs light shade) carry the fill/track contrast, so the caller
    // just tints the whole thing accent-or-urgent (see `ctxColor`).
    function ctxBar(pct, cells) {
        var n = cells || 8
        var filled = Math.round(Math.max(0, Math.min(100, pct)) / 100 * n)
        var s = "["
        for (var i = 0; i < n; i++) s += (i < filled) ? "▓" : "░"
        s += "]"
        return s
    }
    // Fill colour: the song accent, shifting to `paletteUrgent` at/above 85%
    // (a session about to overflow its window reads as urgent at a glance) —
    // the same 85% threshold and accent→urgent swap MetersGadget's CPU/RAM
    // gauges already use.
    readonly property real ctxUrgentAt: 85
    function ctxColor(pct, accent) {
        return pct >= ctxUrgentAt ? paletteUrgent : accent
    }
    // Rollup over a set of session-shaped objects (anything carrying `model` +
    // `contextTokens` — a raw stage record, or a beamed-tree row): the
    // hottest window's fill (MAX, never averaged — a worst-case alarm, not a
    // blended reading) and the raw token SUM. Shared by ConductorGadget's
    // project-header and fleet-footer rollups so both levels compute the
    // exact same two numbers the exact same way. `any` is false when nothing
    // in the set has produced a turn yet, so a caller can omit the gauge
    // entirely rather than render a false 0%.
    function ctxRollup(items) {
        var maxFill = 0, sumTok = 0, any = false
        var list = items || []
        for (var i = 0; i < list.length; i++) {
            var it = list[i]
            var tok = (it && it.contextTokens) || 0
            if (tok <= 0) continue
            any = true
            sumTok += tok
            var pct = ctxPercent(it ? it.model : "", tok)
            if (pct > maxFill) maxFill = pct
        }
        return { any: any, maxFill: maxFill, sumTok: sumTok }
    }

    // ── Usage panel: path + reset-countdown (shared by UsageGadget) ────────
    // The `aoide usage` poller writes ~/Aoide/state/usage.json (schema §0 —
    // live plan/weekly utilization + a local this-machine estimate). Path kept
    // here alongside notePath so a consumer never hard-codes its own copy.
    readonly property string usagePath:
        Quickshell.env("HOME") + "/Aoide/state/usage.json"
    // A compact "resets in 3h20m" from an ISO-8601 instant, relative to `nowMs`
    // (a live-ticking clock the caller threads in so the countdown counts down
    // without a re-read). Empty string for a missing/unparseable stamp, so the
    // caller can omit the clause entirely rather than render a blank.
    function usageResetIn(iso, nowMs) {
        if (!iso) return ""
        var t = Date.parse(iso)
        if (isNaN(t)) return ""
        var now = nowMs || Date.now()
        var d = Math.floor((t - now) / 1000)
        if (d <= 0) return "resets now"
        var h = Math.floor(d / 3600), m = Math.floor((d % 3600) / 60)
        if (h >= 24) { var days = Math.floor(h / 24); return "resets in " + days + "d" + (h % 24) + "h" }
        if (h > 0) return "resets in " + h + "h" + (m < 10 ? "0" : "") + m + "m"
        return "resets in " + m + "m"
    }

    // ── File watcher — atomic hot-reload ──────────────────────────────────
    // Declared as a property (not a default-child) because QtObject has no
    // default property — nesting it directly fails to load.
    property FileView noteFile: FileView {
        id: noteFile
        path: root.notePath
        watchChanges: true
        onFileChanged: noteFile.reload()
        onTextChanged: {
            try {
                var parsed = JSON.parse(noteFile.text())
                root.raw = parsed
            } catch (e) {
                console.warn("[aoide/notes] Failed to parse drachma.json:", e)
            }
        }
        Component.onCompleted: noteFile.reload()
    }
}
