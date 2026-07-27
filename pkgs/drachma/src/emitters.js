// src/emitters.js — the three live-side emitters (concepts/Notes).
//
// All three consume the SAME fully-resolved note set from resolve.js, so the
// three live targets can never disagree. Emitters are minimal but produce
// well-formed output for the v0 schema.
//
//   1. stage    — song/stage/notes.json for Quickshell (CONTRACTS.md §4).
//   2. hyprctl  — `hyprctl` dispatch commands for the compositor.
//   3. osc      — terminal OSC colour sequences.

"use strict";

const { SCHEMA_VERSION } = require("./schema");

// ── 1. stage/notes.json (Quickshell) ───────────────────────────────────────
// The resolved, flattened values. Component fallbacks are already applied by
// resolve.js, so Quickshell reads concrete colours, never null. Shape matches
// CONTRACTS.md §4 exactly.
function emitStage(resolved) {
  const out = {
    schemaVersion: resolved.schemaVersion || SCHEMA_VERSION,
    palette: resolved.palette,
    bar: resolved.bar,
    notif: resolved.notif,
    window: resolved.window,
  };
  // The base16 tier rides through untouched when present (Quickshell reads the
  // wireframe accents from it — NoteState.qml). Omitted when the note lacks it.
  if (resolved.base16) out.base16 = resolved.base16;
  return out;
}

// ── 2. hyprctl dispatcher (compositor) ──────────────────────────────────────
// Window decoration is the compositor's v0 surface. hyprctl expects colours as
// `rgb(rrggbb)` / `rgba(rrggbbaa)`. We emit the two border colours as keyword
// setters. Returns an array of argv-style commands (well-formed, quoting-safe).
function hyprColor(hex) {
  return `rgb(${String(hex).replace(/^#/, "")})`;
}
function emitHyprctl(resolved) {
  const w = resolved.window;
  return [
    ["hyprctl", "keyword", "general:col.active_border", hyprColor(w.border)],
    [
      "hyprctl",
      "keyword",
      "general:col.inactive_border",
      hyprColor(w.borderInactive),
    ],
  ];
}

// ── 3. terminal OSC sequences ───────────────────────────────────────────────
// OSC 10 = foreground, OSC 11 = background, OSC 12 = cursor, and OSC 4 sets
// palette entries. We map the v0 palette onto the conventional slots. Each
// sequence is ESC ] <ps> ; <color> BEL. Returns an array of raw strings.
function oscRgb(hex) {
  const h = String(hex).replace(/^#/, "");
  const r = h.slice(0, 2);
  const g = h.slice(2, 4);
  const b = h.slice(4, 6);
  return `rgb:${r}${r}/${g}${g}/${b}${b}`;
}
function emitOsc(resolved) {
  const BEL = "";
  const ESC = "";
  const p = resolved.palette;
  const seq = (ps, color) => `${ESC}]${ps};${oscRgb(color)}${BEL}`;
  return [
    seq("11", p.bg), // background
    seq("10", p.fg), // foreground
    seq("12", p.accent), // cursor
    // Map the four base colours onto base16-ish ANSI slots (minimal v0 subset).
    `${ESC}]4;0;${oscRgb(p.bg)}${BEL}`, // ansi 0  (black/bg)
    `${ESC}]4;4;${oscRgb(p.accent)}${BEL}`, // ansi 4  (blue/accent)
    `${ESC}]4;1;${oscRgb(p.urgent)}${BEL}`, // ansi 1  (red/urgent)
    `${ESC}]4;7;${oscRgb(p.fg)}${BEL}`, // ansi 7  (white/fg)
  ];
}

module.exports = { emitStage, emitHyprctl, emitOsc };
