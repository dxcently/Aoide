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
