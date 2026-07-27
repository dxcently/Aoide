// src/schema.js — the authoritative v0 note schema (CONTRACTS.md §1).
//
// This is what `drachma lint` (and, transitively, `rice lint`) validates
// against. The nix option type (modules/nucleus/options.nix) is a permissive
// gate; THIS is the authoritative validator.
//
// v0 lives inside a W3C design-tokens container: a note is a W3C design-token
// `$value` (and optionally `$type`). References use the `{group.name}` alias
// syntax that Style Dictionary resolves. The palette tier is base16-closed; the
// component tier is bar.* / notif.* / window.*, each field `nullOr hex` where
// null means "fall back to the palette" (the FACET applies the fallback; the
// stage emitter resolves it fully — see resolve.js).

"use strict";

const HEX = /^#?[0-9a-fA-F]{6}$/;

// The closed v0 shape: which keys exist in each tier, and the component→palette
// fallback map (CONTRACTS.md §1). Component values may also be null.
const PALETTE_KEYS = ["bg", "fg", "accent", "urgent"];

// Optional palette keys — present or absent, but when present must be a hex
// colour (never null). `hot` is the one-neon trace/highlight colour (the
// reference stills' optic-nerve green): notes WITHOUT it stay valid, and a
// facet falls the surface back to `accent` when it is absent. Keeping it
// optional preserves the v0 contract for every existing note file.
const PALETTE_OPTIONAL_KEYS = ["hot"];

// The base16 tier — an OPTIONAL top-level block (sibling of `palette`) carrying
// the full sixteen-slot terminal scheme (the "pantheon bw" ramp + accent set).
// All-or-nothing and CLOSED, exactly like the palette: when the block is present
// every slot base00..base0F must be given (each a hex, never null), and any key
// outside the sixteen is rejected. Notes WITHOUT it stay valid — a facet falls
// the wireframe accents back to `accent` when the block is absent, so the v0
// contract is preserved for every existing note file (same posture as `hot`).
const BASE16_KEYS = [
  "base00", "base01", "base02", "base03",
  "base04", "base05", "base06", "base07",
  "base08", "base09", "base0A", "base0B",
  "base0C", "base0D", "base0E", "base0F",
];

const COMPONENT_FALLBACK = {
  bar: { bg: "bg", fg: "fg", accent: "accent" },
  notif: { bg: "bg", fg: "fg", urgent: "urgent" },
  window: { border: "accent", borderInactive: "bg" },
};

// Accept either a bare hex string or a W3C design-token object ({ $value, $type }).
// A reference like "{palette.bg}" is a valid $value (resolved later).
function noteValue(node) {
  if (node == null) return null;
  if (typeof node === "string") return node;
  if (typeof node === "object" && "$value" in node) return node.$value;
  return undefined; // signals "not a note leaf"
}

function isRef(v) {
  return typeof v === "string" && /^\{[^}]+\}$/.test(v);
}

// Validate a resolved (dereferenced) value: must be hex, or null for a
// component field (null is legal at the schema tier; the emitter resolves it).
function checkColor(value, path, errors, { allowNull }) {
  if (value === null || value === undefined) {
    if (allowNull) return;
    errors.push(`${path}: missing (expected hex #rrggbb)`);
    return;
  }
  if (typeof value !== "string") {
    errors.push(`${path}: expected hex string, got ${typeof value}`);
    return;
  }
  if (isRef(value)) return; // unresolved reference — resolver checks target
  if (!HEX.test(value)) {
    errors.push(`${path}: "${value}" is not a hex colour (#rrggbb)`);
  }
}

// Validate a v0 note container. Returns { ok, errors }. Works on the RAW
// container (references still present) — reference targets are checked
// structurally, full resolution is validated separately after Style Dictionary.
function validate(container) {
  const errors = [];

  if (container == null || typeof container !== "object") {
    return { ok: false, errors: ["root: note file must be a JSON object"] };
  }

  // Palette tier — required and closed.
  const palette = container.palette;
  if (palette == null || typeof palette !== "object") {
    errors.push("palette: required group missing");
  } else {
    for (const k of PALETTE_KEYS) {
      const v = noteValue(palette[k]);
      if (v === undefined) {
        errors.push(`palette.${k}: required`);
      } else {
        checkColor(v, `palette.${k}`, errors, { allowNull: false });
      }
    }
    // Optional keys — validated only when present (never null when given).
    for (const k of PALETTE_OPTIONAL_KEYS) {
      if (palette[k] === undefined) continue;
      const v = noteValue(palette[k]);
      checkColor(v, `palette.${k}`, errors, { allowNull: false });
    }
    for (const k of Object.keys(palette)) {
      if (!PALETTE_KEYS.includes(k) && !PALETTE_OPTIONAL_KEYS.includes(k)) {
        errors.push(`palette.${k}: unknown key (v0 palette is closed)`);
      }
    }
  }

  // Base16 tier — optional, all-or-nothing, closed. Validated only when the
  // block is present; then every slot is required (a hex, never null) and no
  // key outside the sixteen is allowed. Mirrors the palette's closed handling.
  const base16 = container.base16;
  if (base16 !== undefined) {
    if (base16 == null || typeof base16 !== "object") {
      errors.push("base16: expected a note group object");
    } else {
      for (const k of BASE16_KEYS) {
        const v = noteValue(base16[k]);
        if (v === undefined) {
          errors.push(`base16.${k}: required (base16 is all-or-nothing)`);
        } else {
          checkColor(v, `base16.${k}`, errors, { allowNull: false });
        }
      }
      for (const k of Object.keys(base16)) {
        if (!BASE16_KEYS.includes(k)) {
          errors.push(`base16.${k}: unknown key (base16 is closed)`);
        }
      }
    }
  }

  // Component tier — optional groups, each field nullOr hex.
  for (const [group, fields] of Object.entries(COMPONENT_FALLBACK)) {
    const g = container[group];
    if (g == null) continue; // whole group optional
    if (typeof g !== "object") {
      errors.push(`${group}: expected a note group object`);
      continue;
    }
    for (const field of Object.keys(g)) {
      if (!(field in fields)) {
        errors.push(`${group}.${field}: unknown key (v0 ${group} is closed)`);
        continue;
      }
      const v = noteValue(g[field]);
      checkColor(v, `${group}.${field}`, errors, { allowNull: true });
    }
  }

  return { ok: errors.length === 0, errors };
}

module.exports = {
  SCHEMA_VERSION: "0",
  HEX,
  PALETTE_KEYS,
  PALETTE_OPTIONAL_KEYS,
  BASE16_KEYS,
  COMPONENT_FALLBACK,
  noteValue,
  isRef,
  validate,
};
