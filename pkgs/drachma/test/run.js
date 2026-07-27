#!/usr/bin/env node
// test/run.js — drachma v0 schema tests (assert-driven, no framework).
//
// The authoritative validator (src/schema.js) is exercised directly against
// fixtures. Run with `npm test` (package.json wires this) and again in the nix
// installCheckPhase (default.nix), so the contract is machine-checked on every
// build. Exit 0 = all green; exit 1 = a failing assertion (prints which).

"use strict";

const fs = require("fs");
const path = require("path");
const schema = require("../src/schema");

const FIX = path.join(__dirname, "fixtures");
function load(name) {
  return JSON.parse(fs.readFileSync(path.join(FIX, name), "utf8"));
}

let failures = 0;
function check(name, cond) {
  if (cond) {
    process.stdout.write(`ok   — ${name}\n`);
  } else {
    failures++;
    process.stdout.write(`FAIL — ${name}\n`);
  }
}

// 1. The baseline v0 fixture (no `hot`) stays valid — optionality preserved.
{
  const r = schema.validate(load("valid.json"));
  check("valid.json (no hot) is accepted", r.ok);
}

// 2. A palette WITH a well-formed `hot` hex is accepted (hot-accepted).
{
  const r = schema.validate(load("valid-hot.json"));
  check("palette.hot (good hex) is accepted", r.ok);
}

// 3. A palette with a bad `hot` hex is rejected, and the error names it
//    (hot-bad-hex-rejected).
{
  const r = schema.validate(load("invalid-hot.json"));
  check("palette.hot (bad hex) is rejected", !r.ok);
  check(
    "rejection error names palette.hot",
    r.errors.some((e) => e.includes("palette.hot"))
  );
}

// 4. `hot` is truly optional, not part of the required closed set — an
//    unknown *other* key still fails, but `hot` never triggers unknown-key.
{
  const bad = load("valid.json");
  bad.palette.bogus = "#000000";
  const r = schema.validate(bad);
  check("an unrelated unknown palette key is still rejected", !r.ok);
  check(
    "hot is in the optional-key allowlist",
    schema.PALETTE_OPTIONAL_KEYS.includes("hot")
  );
}

// 5. The base16 tier — an optional top-level block. A full sixteen-slot block
//    is accepted (base16-accepted).
{
  const r = schema.validate(load("valid-base16.json"));
  check("valid-base16.json (full sixteen) is accepted", r.ok);
}

// 6. A base16 block missing a slot is rejected, and the error names the slot
//    (base16-missing-slot-rejected).
{
  const r = schema.validate(load("invalid-base16-missing.json"));
  check("base16 missing a slot is rejected", !r.ok);
  check(
    "rejection error names the missing base16 slot",
    r.errors.some((e) => e.includes("base16.base0F"))
  );
}

// 7. A base16 block with a bad hex is rejected, and the error names the slot
//    (base16-bad-hex-rejected).
{
  const r = schema.validate(load("invalid-base16-hex.json"));
  check("base16 with a bad hex is rejected", !r.ok);
  check(
    "rejection error names the bad base16 slot",
    r.errors.some((e) => e.includes("base16.base0F"))
  );
}

// 8. An unknown key INSIDE the base16 block is rejected (base16 is closed), and
//    the sixteen canonical slots are the exported allowlist.
{
  const bad = load("valid-base16.json");
  bad.base16.base10 = { "$type": "color", "$value": "#000000" };
  const r = schema.validate(bad);
  check("an unknown key inside base16 is rejected", !r.ok);
  check("base16 exposes the full sixteen-slot keyset", schema.BASE16_KEYS.length === 16);
}

// 9. base16 stays truly OPTIONAL — the baseline v0 fixture (no base16) is valid.
{
  const r = schema.validate(load("valid.json"));
  check("valid.json (no base16) is still accepted", r.ok);
}

if (failures > 0) {
  process.stderr.write(`\n${failures} test(s) failed\n`);
  process.exit(1);
}
process.stdout.write("\nall drachma schema tests passed\n");
