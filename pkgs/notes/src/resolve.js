// src/resolve.js — the tiered resolver, wrapping Style Dictionary.
//
// We DO NOT hand-roll reference resolution. Style Dictionary owns the
// palette→semantic→component dereferencing (the W3C `{group.name}` alias
// syntax). This module:
//   1. hands the raw v0 container to Style Dictionary to dereference,
//   2. applies the v0 component-tier null→palette fallback (CONTRACTS.md §1),
//   3. returns a flat, fully-resolved note set the emitters consume.
//
// The fallback is applied HERE for the live-side emitters so the stage file
// carries concrete colours (Quickshell never reads null — CONTRACTS.md §4).
// The nix FACET applies the same fallback independently for the baked side;
// both derive from identical rules so preview and adopted state cannot diverge.

"use strict";

// Style Dictionary v4 is ESM-first but ships a CJS interop wrapper: the class
// arrives under `.default` when required from CommonJS.
const _sd = require("style-dictionary");
const StyleDictionary = _sd.default || _sd;
const { COMPONENT_FALLBACK, noteValue, SCHEMA_VERSION } = require("./schema");

function stripHash(v) {
  return typeof v === "string" && v.startsWith("#") ? v.slice(1) : v;
}
function withHash(v) {
  if (typeof v !== "string") return v;
  return v.startsWith("#") ? v : `#${v}`;
}

// Run the container through Style Dictionary purely to dereference `{a.b}`
// aliases. We use its programmatic API with a no-op in-memory platform and read
// back the dictionary of resolved notes. Style Dictionary v4 is async.
async function dereference(container) {
  const sd = new StyleDictionary({
    tokens: container,
    // A minimal platform; we never write files, we read `.allTokens` back.
    platforms: {
      _resolve: { transformGroup: "js", buildPath: "/dev/null/" },
    },
    log: { verbosity: "silent", warnings: "disabled" },
  });

  // exportPlatform resolves references and returns the note tree for a
  // platform without touching the filesystem.
  const resolved = await sd.exportPlatform("_resolve");
  return resolved;
}

// Pull a leaf value out of the (possibly W3C-wrapped) resolved tree.
function leaf(node) {
  return withHash(noteValue(node));
}

// Produce the fully-resolved, flattened note set. Component fields absent or
// null fall back to their palette source per COMPONENT_FALLBACK.
async function resolve(container) {
  const tree = await dereference(container);

  const palette = {};
  for (const [k, node] of Object.entries(tree.palette || {})) {
    palette[k] = leaf(node);
  }

  const out = { schemaVersion: SCHEMA_VERSION, palette };

  for (const [group, fields] of Object.entries(COMPONENT_FALLBACK)) {
    out[group] = {};
    const g = tree[group] || {};
    for (const [field, paletteKey] of Object.entries(fields)) {
      const raw = noteValue(g[field]);
      out[group][field] =
        raw == null || raw === "" ? palette[paletteKey] : withHash(raw);
    }
  }

  return out;
}

module.exports = { resolve, dereference, stripHash, withHash };
