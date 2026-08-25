---
type: concept
created: 2026-08-19
updated: 2026-08-25
tags: [aoide, cli, meta, upkeep]
---

# Meta & Upkeep Verbs — Guide, Schema, Soundcheck

The meta verbs (`guide`, `schema`) orient an agent. The stubs (`make`,
`update`, `onboard`) are walking-skeleton reservations of schema surface for
planned flows. The upkeep verbs (`usage`, `quickshell reload`, `soundcheck`)
maintain local state and sweep the working tree. Handlers live in
`pkgs/aoide/crates/cli/src/commands/meta.rs` (`guide`/`schema`),
`pkgs/aoide/crates/cli/src/commands/stubs.rs` (the stubs),
`pkgs/aoide/crates/storage/src/commands.rs` (`usage`),
`pkgs/aoide/crates/song/src/commands/quickshell.rs` (`quickshell reload`),
and `pkgs/aoide/crates/upkeep/src/{commands,scan}.rs` (`soundcheck`).

Every command takes `--json`. Without it the CLI prints the human `message`
line; with it, an envelope `{status, command, message, gated, changed?, data?}`
(`pkgs/aoide/crates/protocol/src/output.rs`). Exit codes: 0 ok, 1 error,
2 usage, 64 not-implemented. Every dispatch — stubs included — appends one
line to the audit log at `~/Aoide/log` (`$AOIDE_AUDIT_LOG` or a `--audit-log`
flag overrides; `pkgs/aoide/crates/protocol/src/audit.rs`).

### aoide guide

```
aoide guide [--json]
```

- **Output:** the tier-0 onboarding text (the four-tier map + house rules) —
  a compiled-in constant (`pkgs/aoide/crates/cli/src/guide.rs`) mirroring
  `AGENTS.md`. Message `"printed the four-tier onboarding"`; `--json` data
  `{text: <full guide>}`.
- **Notes:** read-only; not gated. See [[Agent-Interface]] for the tier model
  the guide describes. `lyra guide` is the paint-side mirror — its own
  compiled-in constant (`pkgs/aoide/crates/lyra/src/guide.rs`), reading
  lyra's own registry rather than core's.

### aoide schema

```
aoide schema [--json]
```

- **Output:** message `"emitted the v0 command + state-file schema"`;
  `--json` data is the full schema document built from the in-process command
  registry (`Registry::schema`,
  `pkgs/aoide/crates/protocol/src/registry.rs`). Top-level keys:
  `{schemaVersion, aoide, stageNotesVersion, commands}` — despite the
  summary's "state file" phrasing, the emitted document carries commands
  only, no separate state-file section. Each `commands[]` entry:
  `{path, summary, args, flags, gated, implemented, exitCodes}` (plus
  `examples` where declared).
- **Notes:** read-only; not gated. This document is the machine-readable
  backstop every tier generates from — the MCP tool list derives from it
  ([[Agent-Interface]]). `lyra schema` mirrors it for the paint side: its own
  registry, its own golden snapshot (42 command paths, evolving independently
  of core's 69 — see [[lyra]]).

### aoide make

```
aoide make <intent> [--json]
```

- **Notes:** STUB — `implemented: false`, `gated: false`
  (`pkgs/aoide/crates/cli/src/commands/stubs.rs::register_make`). Dispatch
  short-circuits before any handler: `status: "not-implemented"`, exit 64,
  message "`aoide make` is a walking-skeleton stub: arg-parsing and schema
  are real, the live-system action is not yet implemented.", data `{path,
  args, flags}`. Contract per the schema summary: the widget-maker entry —
  generate a dendrite + widget + adapter from a natural-language `<intent>`
  (e.g. "show my scheduled jobs"). See [[Widget-Maker]].

### aoide update

```
aoide update [--check-only] [--json]
```

- **Notes:** STUB — `implemented: false`, `gated: true`
  (`stubs.rs::register_update`). Same not-implemented envelope and exit 64 as
  `make`, with `gated: true` in the envelope. Contract per the schema summary:
  fetch upstream, merge framework paths, run checks, propose the rebuild —
  the user admits, the agent only proposes ([[Rebuild-Gate]]). `--check-only`
  limits the run to detecting contract bumps; no merge.

### aoide onboard

```
aoide onboard [--json]
```

- **Notes:** STUB — `implemented: false`, `gated: false`
  (`stubs.rs::register_onboard`). Same not-implemented envelope and exit 64.
  Contract per the schema summary: the first-boot flow — register the clone,
  seed the songbook, print the guide. See [[Clone-and-Run]].

### aoide usage

```
aoide usage [--json]
```

- **Reads:** every `*.jsonl` under `~/.claude/projects/` recursively
  (Claude Code transcripts, including `subagents/`; only `type:"assistant"`
  lines with a `message.usage` block count — override:
  `$AOIDE_CLAUDE_PROJECTS_DIR`). For the live block:
  `~/.claude/.credentials.json` (`claudeAiOauth.accessToken`; override:
  `$AOIDE_CLAUDE_CREDENTIALS`). Env: `$AOIDE_USAGE_UA` (User-Agent override,
  sanitized of quotes/backslashes/control chars), `$AOIDE_STATE_DIR`, `$HOME`.
  Spawns `claude --version` (to build the `claude-code/<ver>` User-Agent;
  fallback `claude-code/2.1.0`).
- **Writes:** `state/usage.json` (resolves to `~/Aoide/state/usage.json`;
  atomic temp-then-rename, symlink-transparent —
  `aoide_storage::fs::atomic_write`). Body: `{schemaVersion: "0", fetchedAt,
  live, local}` where `local` is `{note, today: {tokens, costUsd}, week:
  {tokens, costUsd}}` and `live` is `{ok, error?, fiveHour?, sevenDay?,
  sevenDayOpus?, sevenDaySonnet?, extraUsage?}` (meters are `{utilization,
  resetsAt?}`; on `ok:false` only `{ok, error}` serialize).
- **Pipes to / output:** a best-effort HTTPS fetch of the unofficial
  `https://api.anthropic.com/api/oauth/usage` endpoint via `curl -sS
  --max-time 15 -w '\n%{http_code}' --config -` — the curl config carrying
  the Bearer token is piped over stdin, so the token never touches argv or a
  file and is never logged or written. ANY failure (missing credentials,
  401/403, transport error, unparseable body) degrades to `live.ok:false`
  with a tokenless reason. Human line: `"today N tokens (~$X.XX, local
  estimate); week N tokens (~$Y.YY) — state/usage.json written"`; `--json`
  data is the written body; `changed` lists the target path.
- **Notes:** not gated. The `local` rollup is an estimate: embedded
  per-model-family pricing (USD/MTok; unknown models fall back to the
  Sonnet-tier rate), `today` is bounded at UTC midnight, `week` is the
  trailing 7 days (a superset of `today`). Missing/unreadable transcript dirs
  yield zeros, never an error. Exit 1 only when the state write itself fails
  (`reason: "state-write-failed"`). The usage widget ([[Gadget-Dock]]) reads
  the written file and must tolerate `live.ok == false`.

### lyra quickshell reload

```
lyra quickshell reload [--json]
```

- **Reads:** liveness probe via `systemctl --user show
  aoide-quickshell.service --property=MainPID --value`
  (`pkgs/aoide/crates/song/src/reap.rs`); the `shell.qml` path under
  `run/qml/` (resolves to `~/Aoide/run/qml/shell.qml`;
  `$AOIDE_STAGE_DIR`-relocatable).
- **Pipes to / output:** when the service is up, spawns `quickshell -p
  <run_qml_dir>/shell.qml ipc call shell reload` — the `-p` is required
  because `ipc call` does not auto-discover an instance launched with a
  path config. This invokes the `Quickshell.reload(false)` IPC handler
  (`AoideIpc.qml`), which tears down and rebuilds the whole scene from
  `shell.qml` — no systemd restart. Success is judged on OUTPUT, not the
  exit code: `ipc call` exits 0 even for an unknown target/function
  (printing `Not ready to accept queries yet.`), and `reload()` is a void
  function, so a landed call prints nothing — empty stdout at exit 0 is the
  only real success. Always `status: "ok"`; `--json` data `{status:
  "reloaded" | "not-running" | "failed"}` with the matching human message.
- **Notes:** not gated; best-effort — `not-running` (service absent: no IPC
  attempted) and `failed` are reported facts, never command failures; exit
  stays 0. This is the reload lane for dynamically-loaded widget QML
  (`Qt.createComponent`) and facet-owned QML that Quickshell's own file
  watcher never tracks ([[Quickshell]]). Named `quickshell`, not `shell`,
  because a top-level `shell` command collided with the `--agent shell`
  flag value (see the module doc in
  `pkgs/aoide/crates/song/src/commands/quickshell.rs`).

### aoide soundcheck

```
aoide soundcheck [--json]
```

- **Reads:** the repo root (`~/Aoide`; derived as `song_dir()`'s parent, so
  `$AOIDE_STAGE_DIR` relocates it). Shells out to git: `git status
  --porcelain` (C1), `git ls-files --cached --ignored --exclude-standard`
  (C2), `git check-ignore -v --no-index <path>` (C2's `assertedAt`
  resolution), plus `read_dir`/`readlink` on the root (C3). All in
  `pkgs/aoide/crates/upkeep/src/scan.rs`.
- **Writes:** none — report-only, forever; never mutates, moves, or deletes.
- **Output:** `"clean — no findings"` or `"N finding(s) (E error, I info) —
  see --json for detail"`. `--json` data: `{findings: [{id, check, severity,
  path, rule, assertedAt, detail, action}], summary: {unrecognized,
  "orphan-tracked", clutter}}`. `id` is stable across runs
  (`<check>:<path-slug>`, `/` and `.` → `-`); `action` is a concrete command
  or instruction, never a promise the tool will do it.
- **Notes:** not gated. Three check classes: C1 `unrecognized` (error) — a
  root-level entry neither tracked nor gitignored (`assertedAt`:
  "CONTRACTS.md §2 — the repo root is closed"); C2 `orphan-tracked` (error)
  — a file git tracks that `.gitignore` also matches, anywhere in the tree,
  with `action: "git rm --cached <path>"`; C3 `clutter` (info) — a root
  symlink into `/nix/store` (`result`, `result-*`). Any error-severity
  finding fails the run at exit 1 (so `aoide soundcheck && git commit`
  gates); info findings never do. Outside a git repo the git-sourced checks
  degrade to no findings; C3 (filesystem-only) still runs. The
  committed-tree half (fmt/lint/discovery) is owned by `nix flake check`
  (`lib/checks.nix`) — no overlap. C4–C6 (`fmt`/`lint`/`tool-missing`
  shell-outs) are named in source comments as a later step and are not
  implemented.

## Related

- [[Agent-Interface]]
- [[Widget-Maker]]
- [[Clone-and-Run]]
- [[Rebuild-Gate]]
- [[Quickshell]]
- [[Gadget-Dock]]
- [[aoide-cli]]
