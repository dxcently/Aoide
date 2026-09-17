---
type: concept
created: 2026-08-19
updated: 2026-08-29
tags: [aoide, cli, meta, upkeep]
---

# Meta & Upkeep Commands — Guide, Schema, Soundcheck

The meta commands (`guide`, `schema`) orient an agent. The stubs (`make`,
`update`) are walking-skeleton reservations of schema surface for planned
flows. The upkeep commands (`usage`, `quickshell reload`/`healthcheck`,
`soundcheck`) maintain local state and sweep the working tree; `onboard`
runs the first-boot install flow. Handlers live in
`pkgs/aoide/crates/cli/src/commands/meta.rs` (`guide`/`schema`),
`pkgs/aoide/crates/cli/src/commands/stubs.rs` (the stubs),
`pkgs/aoide/crates/cli/src/commands/onboard.rs` (`onboard`),
`pkgs/aoide/crates/storage/src/commands.rs` (`usage`),
`pkgs/aoide/crates/song/src/commands/quickshell.rs` (`quickshell
reload`/`healthcheck`, the latter over
`pkgs/aoide/crates/song/src/health.rs`), and
`pkgs/aoide/crates/upkeep/src/{commands,scan}.rs` (`soundcheck`).

Every command takes `--json`. Without it the CLI prints the human `message`
line; with it, an envelope `{status, command, message, gated, changed?, data?}`
(`pkgs/aoide/crates/protocol/src/output.rs`). Exit codes: 0 ok, 1 error,
2 usage, 64 not-implemented. Every dispatch — stubs included — appends one
line to the audit log (`default_audit_log`: `$AOIDE_AUDIT_LOG` non-empty,
else `$AOIDE_ROOT/log`, default `~/.aoide/log`; a `--audit-log` flag
overrides; `pkgs/aoide/crates/protocol/src/audit.rs`).

### aoide guide

```
aoide guide [--json]
```

- **Output:** the tier-0 onboarding text — identity (core vs paint), the
  four-tier map, a command-surface table, and the nine house-rule titles,
  each with a pointer to its canonical home: rule bodies in the repo's root
  `AGENTS.md` (same numbering), the read order in `docs/agent/README.md`,
  long forms in the wiki. The prose is compiled in
  (`pkgs/aoide/crates/cli/src/guide.rs`) — the crate's build source excludes
  `docs/`, so the seam is the pointer, not an `include_str!` — while the
  table (per group: command count, stub tally where nonzero; one total line)
  is derived at print time from the registry the binary assembled at boot,
  in registry order. It restates no rule body and hand-lists no command;
  `schema --json` owns the command surface, and the table reads the same
  `Registry` instance it emits. Message `"printed the four-tier
  onboarding"`; `--json` data `{text: <full guide>}`.
- **Notes:** read-only; not gated. See [[Agent-Interface]] for the tier model
  the guide describes. `lyra guide` is the paint-side mirror — the same
  shape from its own `pkgs/aoide/crates/lyra/src/guide.rs` against lyra's
  own registry, scoped to lyra's side of the boundary.

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
  registry, its own golden snapshot (48 command paths, evolving independently
  of core's 80 — see [[lyra]]).

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
aoide onboard [--harness <a,b,...>] [--yes] [--out <path>] [--json]
```

- **Reads:** the checkout root, found by walking up from the cwd to
  `.claude/skills/aoide/SKILL.md` (`hooks::skill_source()`'s walk-up);
  outside a checkout the run fails exit 1, `reason: "no-checkout"`. PATH
  probes for each known harness (`claude`, `kimi`, `pi` via
  `aoide_protocol::agents`) and for the lyra binary (`rice_bin()`:
  `$AOIDE_RICE_BIN`, a sibling binary, then PATH).
- **Writes:** registers the clone by dispatching the already-registered
  `project add aoide <checkout>` handler (stage-file effects per
  [[Graph-and-Conduct]]); links `~/song` → `<checkout>/song` when nothing is
  there (a correct existing link is a no-op; a wrong-target symlink or a
  non-symlink is left alone with a note, never clobbered); seeds
  `song/songbook/preferences.md` when absent (created once, never
  overwritten). For each chosen harness, runs the registered
  `hooks install <agent>` handler (harness settings merge and skill symlink
  per [[Content-and-Hooks]]).
- **Pipes to / output:** progress lines per phase; the closing output is the
  full `aoide guide` text rendered from the same assembled registry. Message
  `"onboard: clone registered, N harness(es) wired -- <lyra note>"`;
  `--json` data `{root, harnesses, hooks: [{agent, ok, message}], lyra}`.
- **Notes:** not gated; `implemented: true`, registered in
  `pkgs/aoide/crates/cli/src/commands/onboard.rs`. CLI door only — any other
  door is a usage error (exit 2). Harness selection: `--harness` wires
  exactly the comma-separated list (an unknown name is a usage error); given
  neither flag, a tty gets the interactive multi-select preselected by the
  PATH probe, while `--yes` or a non-tty run wires every harness found on
  PATH. When lyra resolves, the nix half is delegated to a child `lyra
  onboard` with inherited stdio (`--out` and `--yes` forward; `--harness`
  does not) — a spawn failure or nonzero exit degrades to a note, not an
  onboard failure. See [[Clone-and-Run]].

### lyra onboard

```
lyra onboard [--out <path>] [--yes] [--json]
```

- **Reads:** the checkout root, found by walking up from the cwd for a
  directory holding both `flake.nix` and `pkgs/aoide`; outside a checkout the
  run fails exit 1, `reason: "no-checkout"`. The option set is derived live
  with `nix eval <checkout>#aoideOptions` (142 options today), never from a
  hand-list.
- **Writes:** `./aoide.nix` (`--out <path>` overrides) — a nix module the
  user imports, listing every `aoide.*` module option commented out at its
  current default with a one-line description each, plus a commented
  env-knob appendix (`AOIDE_CONDUCT_AUTOGATE`, `AOIDE_TERMINAL`,
  `AOIDE_CORE_BIN`/`AOIDE_RICE_BIN`, `AOIDE_DISCOVERY_ADVERTISE`). Re-running
  over a file it generated warns and backs the old file up to `<out>.bak`
  (an interactive confirm unless `--yes`); a file it did not generate is
  refused, never overwritten.
- **Pipes to / output:** prints the teaching line `imports = [ ./aoide.nix
  ];` — the user's flake is never edited. Message `"lyra onboard: wrote
  <out> (N options) -- imports = [ ... ];"` (`regenerated` on a rerun);
  `--json` data `{out, optionCount, backedUp, importsLine}`.
- **Notes:** not gated; `implemented: true`, registered in
  `pkgs/aoide/crates/lyra/src/commands/onboard.rs`. CLI door only, from a
  checkout — the derivation needs the repo's `modules/` and `flake.nix`. In
  the normal flow this runs as `aoide onboard`'s delegate child, but it
  stands alone. See [[lyra]].

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
- **Writes:** `state/usage.json` under the state dir (`$AOIDE_STATE_DIR`
  absolute override, else `$AOIDE_ROOT/state/` — default
  `~/.aoide/state/usage.json`; atomic temp-then-rename, symlink-transparent —
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
  `run/qml/` (`run_qml_dir()`: `$AOIDE_ROOT/run/qml/shell.qml`, default
  `~/.aoide/run/qml/shell.qml`; `$AOIDE_STAGE_DIR`-relocatable).
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

### lyra quickshell healthcheck

```
lyra quickshell healthcheck [--json]
```

- **Reads:** `systemctl --user show aoide-quickshell.service
  --property=ActiveEnterTimestamp --value` (absent/not-running is
  `HealthOutcome::Healthy` — nothing to watch); the journal since that
  timestamp (`journalctl --user -u aoide-quickshell.service --since
  <timestamp>`) for Qt's placeholder-screen line; `hyprctl -j layers` for the
  live surface count. A restart-history marker at
  `state/quickshell-healthcheck-restarts` under the state dir
  (`pkgs/aoide/crates/song/src/health.rs`).
- **Writes:** the marker file (restart timestamps plus an optional
  `notified:<epoch>` sentinel), and — on a confirmed, ladder-permitted
  lockup — restarts `aoide-quickshell.service` via `systemctl --user
  restart`.
- **Pipes to / output:** always `status: "ok"`; `--json` data `{status:
  "healthy" | "blank" | "restarted" | "deferred"}` with a matching human
  message (a `deferred` message names the seconds until the next attempt and
  the restart count in the last hour). Meant to run off
  `aoide-quickshell-healthcheck.timer`, not interactively.
- **Notes:** not gated; best-effort throughout — a failed
  `systemctl`/`journalctl`/`hyprctl` call reads as `healthy` (nothing
  confirmed), never as a command failure. The two signals are asked in
  order: a live `hyprctl layers -j` reading is compared against the active
  song's declared surfaces (`run/qml/songs/surfaces.json`, per-output where
  declared `perMonitor`; a host whose declaration is absent or empty — every
  song that declares nothing publishes an empty one — falls back to "zero
  `aoide-*` surfaces anywhere"), and a shortfall decides `healthy` on
  its own; the journal's placeholder-screen line (scoped to the unit's own
  `ActiveEnterTimestamp`, so a recovered occurrence can never re-trigger)
  then decides whether the watchdog may act. A shortfall with that line is
  the lockup and is restarted; a shortfall without it is `blank` — reported,
  never restarted, because the placeholder screen is the only mechanism a
  restart is known to undo. With no real output enabled the check stands
  down. See [[Quickshell]]'s "Session service & resilience" for
  the full detection and retry-ladder reasoning. Restarts are throttled on a
  retry ladder (0s/15s/60s/300s, floor 900s, indexed by restarts in the
  trailing hour) that never stops trying — the floor repeats indefinitely.

### aoide soundcheck

```
aoide soundcheck [--json]
```

- **Reads:** the flake checkout root (`flake_root()`: `$AOIDE_FLAKE_ROOT`
  absolute override, else `<home>/Aoide` — the dev git checkout, not the
  runtime root). Shells out to git: `git status
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
