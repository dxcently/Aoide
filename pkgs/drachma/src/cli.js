#!/usr/bin/env node
// src/cli.js — the `drachma` binary (the Aoide note engine).
//
// Subcommands:
//   lint    <drachma.json>            validate against v0 schema (rice lint uses this)
//   resolve <drachma.json>            print the fully-resolved flat note set (JSON)
//   emit stage   <drachma.json> [--out PATH]   write/print stage/drachma.json (atomic)
//   emit hyprctl <drachma.json>       print hyprctl dispatch commands
//   emit osc     <drachma.json>       print terminal OSC colour sequences
//
// Exit codes (aligned with the Aoide CLI convention, CONTRACTS.md §3):
//   0 ok · 2 usage · 1 error (validation failure / bad input)
//
// The `aoide` CLI shells out to this binary; the derivation exposes it as
// `bin/drachma` and `passthru.mainProgram`.

"use strict";

const fs = require("fs");
const os = require("os");
const path = require("path");

const schema = require("./schema");
const { resolve } = require("./resolve");
const { emitStage, emitHyprctl, emitOsc } = require("./emitters");

const EXIT = { OK: 0, USAGE: 2, ERROR: 1 };

function fail(code, msg) {
  process.stderr.write(`drachma: ${msg}\n`);
  process.exit(code);
}

function readNotes(file) {
  if (!file) fail(EXIT.USAGE, "missing <drachma.json> argument");
  let text;
  try {
    text = fs.readFileSync(file, "utf8");
  } catch (e) {
    fail(EXIT.ERROR, `cannot read ${file}: ${e.message}`);
  }
  try {
    return JSON.parse(text);
  } catch (e) {
    fail(EXIT.ERROR, `invalid JSON in ${file}: ${e.message}`);
  }
}

// Atomic write: write to a temp file in the same dir, then rename over target
// (CONTRACTS.md §4 — a hot-reload never reads a torn file).
function atomicWriteJson(target, obj) {
  const dir = path.dirname(path.resolve(target));
  fs.mkdirSync(dir, { recursive: true });
  const tmp = path.join(dir, `.notes.${process.pid}.${Date.now()}.tmp`);
  fs.writeFileSync(tmp, JSON.stringify(obj, null, 2) + "\n");
  fs.renameSync(tmp, target);
}

function cmdLint(argv) {
  const container = readNotes(argv[0]);
  const { ok, errors } = schema.validate(container);
  if (ok) {
    process.stdout.write(
      JSON.stringify({ ok: true, schemaVersion: schema.SCHEMA_VERSION }) + "\n"
    );
    process.exit(EXIT.OK);
  }
  process.stdout.write(JSON.stringify({ ok: false, errors }) + "\n");
  process.exit(EXIT.ERROR);
}

async function cmdResolve(argv) {
  const container = readNotes(argv[0]);
  const { ok, errors } = schema.validate(container);
  if (!ok) {
    process.stdout.write(JSON.stringify({ ok: false, errors }) + "\n");
    process.exit(EXIT.ERROR);
  }
  const resolved = await resolve(container);
  process.stdout.write(JSON.stringify(resolved, null, 2) + "\n");
}

async function cmdEmit(argv) {
  const target = argv[0];
  if (!target) fail(EXIT.USAGE, "emit: missing target (stage|hyprctl|osc)");
  const rest = argv.slice(1);
  const outFlag = rest.indexOf("--out");
  const outPath = outFlag >= 0 ? rest[outFlag + 1] : null;
  const posArgs = rest.filter((a, i) => {
    if (outFlag >= 0 && (i === outFlag || i === outFlag + 1)) return false;
    return true;
  });

  const container = readNotes(posArgs[0]);
  const { ok, errors } = schema.validate(container);
  if (!ok) {
    process.stdout.write(JSON.stringify({ ok: false, errors }) + "\n");
    process.exit(EXIT.ERROR);
  }
  const resolved = await resolve(container);

  switch (target) {
    case "stage": {
      const stage = emitStage(resolved);
      if (outPath) {
        atomicWriteJson(outPath, stage);
        process.stdout.write(
          JSON.stringify({ ok: true, wrote: outPath }) + "\n"
        );
      } else {
        process.stdout.write(JSON.stringify(stage, null, 2) + "\n");
      }
      break;
    }
    case "hyprctl": {
      const cmds = emitHyprctl(resolved);
      // Print one shell-ready command per line.
      for (const argvLine of cmds) {
        process.stdout.write(argvLine.map(shellQuote).join(" ") + "\n");
      }
      break;
    }
    case "osc": {
      const seqs = emitOsc(resolved);
      for (const s of seqs) process.stdout.write(s);
      break;
    }
    default:
      fail(EXIT.USAGE, `emit: unknown target "${target}"`);
  }
}

function shellQuote(s) {
  return /^[A-Za-z0-9_.:\/=()-]+$/.test(s) ? s : `'${s.replace(/'/g, "'\\''")}'`;
}

function usage() {
  process.stdout.write(
    [
      "drachma — Aoide note engine (v0)",
      "",
      "Usage:",
      "  drachma lint    <drachma.json>",
      "  drachma resolve <drachma.json>",
      "  drachma emit stage   <drachma.json> [--out PATH]",
      "  drachma emit hyprctl <drachma.json>",
      "  drachma emit osc     <drachma.json>",
      "",
    ].join("\n")
  );
}

async function main() {
  const [, , cmd, ...argv] = process.argv;
  switch (cmd) {
    case "lint":
      return cmdLint(argv);
    case "resolve":
      return cmdResolve(argv);
    case "emit":
      return cmdEmit(argv);
    case "-h":
    case "--help":
    case undefined:
      usage();
      return process.exit(cmd === undefined ? EXIT.USAGE : EXIT.OK);
    default:
      fail(EXIT.USAGE, `unknown command "${cmd}"`);
  }
}

main().catch((e) => fail(EXIT.ERROR, e.stack || String(e)));
