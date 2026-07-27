// NoteState.qml — shared note state, hot-reloaded from stage/notes.json.
//
// Singleton: every surface widget binds to properties here. When notes.json
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
    // Stage path: ~/Aoide/song/stage/notes.json (gitignored runtime; the nix
    // build never depends on this path — checks.no-song-read enforces that).
    readonly property string notePath:
        Quickshell.env("HOME") + "/Aoide/song/stage/notes.json"

    // ── Parsed note object ─────────────────────────────────────────────────
    property var raw: ({
        "schemaVersion": "0",
        "palette": { "bg": "#1e1e2e", "fg": "#cdd6f4", "accent": "#89b4fa", "urgent": "#f38ba8" },
        "bar":    { "bg": "#1e1e2e", "fg": "#cdd6f4", "accent": "#89b4fa" },
        "notif":  { "bg": "#1e1e2e", "fg": "#cdd6f4", "urgent": "#f38ba8" },
        "window": { "border": "#89b4fa", "borderInactive": "#1e1e2e" }
    })

    // ── Palette shortcuts ──────────────────────────────────────────────────
    readonly property color paletteBg:     raw.palette ? raw.palette.bg     : "#1e1e2e"
    readonly property color paletteFg:     raw.palette ? raw.palette.fg     : "#cdd6f4"
    readonly property color paletteAccent: raw.palette ? raw.palette.accent : "#89b4fa"
    readonly property color paletteUrgent: raw.palette ? raw.palette.urgent : "#f38ba8"
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
                console.warn("[aoide/notes] Failed to parse notes.json:", e)
            }
        }
        Component.onCompleted: noteFile.reload()
    }
}
