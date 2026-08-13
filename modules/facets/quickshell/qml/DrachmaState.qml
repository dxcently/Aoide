// DrachmaState.qml — shared note state, hot-reloaded from stage/livery.json.
//
// Singleton: every surface widget binds to properties here. When the note
// file is atomically replaced (write-temp-then-rename per CONTRACTS.md §4),
// the FileView fires a change signal and all bindings update in one pass —
// the full arrangement hot-reloads without a QML restart. The canonical file
// is livery.json; the legacy drachma.json mirror (LIVERY-MERGE.md §2.3) is
// the fallback, so a fresh shell that starts before any new write still
// finds the file the old seed left. The singleton FILE name stays
// DrachmaState.qml for now — only the stage path constant switched; the QML
// file rename is deferred.
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
    // Stage path: ~/Aoide/song/stage/livery.json (gitignored runtime; the nix
    // build never depends on this path — checks.no-song-read enforces that).
    // The legacy mirror stage/drachma.json (LIVERY-MERGE.md §2.3) is the
    // fallback: a pre-livery writer/reader set keeps rendering until the
    // first livery write lands.
    readonly property string notePath:
        Quickshell.env("HOME") + "/Aoide/song/stage/livery.json"
    readonly property string legacyNotePath:
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
    // The optional top-level base16 block (livery's base16 tier) carries the
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

    // The published per-session ceiling (SessionRecord.contextCeiling, plumbed
    // through sessions.json / graph.json — CONTRACTS.md §4). aoide computes it from
    // the model; the widget just reads it. The 200k default is ONLY a last-resort
    // fallback for legacy records written before this field existed.
    function ctxCeiling(ceiling) {
        return (ceiling && ceiling > 0) ? ceiling : 200000
    }
    function ctxPercent(tokens, ceiling) {
        var t = tokens || 0
        if (t <= 0) return 0
        var pct = t / ctxCeiling(ceiling) * 100
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
    // Rollup over a set of session-shaped objects (anything carrying
    // `contextTokens` + `contextCeiling` — a raw stage record, or a beamed-tree
    // row): the hottest window's fill (MAX, never averaged — a worst-case alarm,
    // not a blended reading) and the raw token SUM. Shared by ConductorGadget's
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
            var pct = ctxPercent(tok, it ? it.contextCeiling : 0)
            if (pct > maxFill) maxFill = pct
        }
        return { any: any, maxFill: maxFill, sumTok: sumTok }
    }

    // ── Elapsed-since (shared by ConductorGadget/TerminalsGadget rosters) ──
    // A compact "how long has this session lived" from its ISO startedAt,
    // relative to `nowMs` (a live-ticking clock the caller threads in, same
    // idiom as usageResetIn below). Empty string for a missing/unparseable
    // stamp so the caller can omit the tally rather than render a blank.
    //   < 1m   → "42s"
    //   < 1h   → "8m09s"
    //   < 24h  → "7h08m"
    //   ≥ 24h  → "2d03h"
    function elapsedSince(iso, nowMs) {
        if (!iso) return ""
        var t = Date.parse(iso)
        if (isNaN(t)) return ""
        var d = Math.floor(((nowMs || Date.now()) - t) / 1000)
        if (d < 0) d = 0
        var h = Math.floor(d / 3600)
        var m = Math.floor((d % 3600) / 60)
        var s = d % 60
        if (h >= 24) {
            var days = Math.floor(h / 24)
            var hr = h % 24
            return days + "d" + (hr < 10 ? "0" : "") + hr + "h"
        }
        if (h > 0) return h + "h" + (m < 10 ? "0" : "") + m + "m"
        if (m > 0) return m + "m" + (s < 10 ? "0" : "") + s + "s"
        return s + "s"
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

    // ── File watcher — atomic hot-reload with legacy fallback ─────────────
    // Declared as a property (not a default-child) because QtObject has no
    // default property — nesting it directly fails to load. The canonical
    // FileView watches livery.json; the legacy FileView watches the
    // drachma.json mirror and only supplies `raw` while the canonical file
    // has not loaded yet (a fresh shell before the first livery write) — once
    // the canonical loads it is the sole source (Phase 4 drops this watcher
    // with the mirror).
    property bool canonicalLoaded: false
    property FileView noteFile: FileView {
        id: noteFile
        path: root.notePath
        watchChanges: true
        onFileChanged: noteFile.reload()
        onTextChanged: {
            var txt = noteFile.text()
            if (!txt) return
            try {
                var parsed = JSON.parse(txt)
                root.canonicalLoaded = true
                root.raw = parsed
            } catch (e) {
                console.warn("[aoide/notes] Failed to parse livery.json:", e)
            }
        }
        Component.onCompleted: noteFile.reload()
    }
    property FileView legacyNoteFile: FileView {
        id: legacyNoteFile
        path: root.legacyNotePath
        watchChanges: true
        onFileChanged: legacyNoteFile.reload()
        onTextChanged: {
            var txt = legacyNoteFile.text()
            if (!txt) return
            try {
                var parsed = JSON.parse(txt)
                if (!root.canonicalLoaded) root.raw = parsed
            } catch (e) {
                console.warn("[aoide/notes] Failed to parse legacy drachma.json:", e)
            }
        }
        Component.onCompleted: legacyNoteFile.reload()
    }
}
