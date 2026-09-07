---
type: concept
created: 2026-08-19
updated: 2026-08-29
tags: [aoide, cli, reference, schema]
---

# CLI Reference — aoide Command I/O Index

Dev-facing reference for the `aoide` and `lyra` command surfaces: for every command, what
it reads, what files it writes, and where its output pipes to. Grounded in the
Rust source (`pkgs/aoide/crates/`), `aoide schema --json`, and `lyra schema
--json` — the pages state
the system at HEAD, not the design intent. For the concepts behind the commands,
start at [[aoide-cli]] and the group pages linked below.

## Conventions that apply to every command

- **Exit codes:** `0` ok · `1` error · `2` usage · `64` not-implemented.
  A stub returns a structured `not-implemented` envelope, never a crash.
- **`--json` everywhere:** every command takes and emits a structured JSON
  envelope; without it, a human line goes to stdout.
- **Audit:** every dispatch — CLI, MCP, or A2A door — appends a JSON-lines
  record to the audit log: `--audit-log` flag, else `$AOIDE_AUDIT_LOG`
  (non-empty), else `$AOIDE_ROOT/log`, default `~/.aoide/log`
  (`default_audit_log`/`audit_log_path`,
  `pkgs/aoide/crates/protocol/src/audit.rs`).
- **Gating:** exactly three commands carry `gated: true` (`rice declare`,
  `content approve`, `update`) — the user rebuild gate ([[Rebuild-Gate]]).
  `send` and `screen send` hold pending approval internally instead;
  `secrets exec` holds the same way when a TOTP code is required and
  absent — the connection PARKS until a separate `secrets approve`/
  `dismiss` call or a timeout, rather than carrying `gated: true`
  ([[Secrets-Broker]]).
- **Runtime root:** the `state/`, `song/stage/`, and `run/qml/` paths below
  hang off one root — `$AOIDE_ROOT` when set to an absolute path, default
  `<home>/.aoide` (`aoide_storage::fs::root`). Per-tree absolute overrides:
  `state/` via `$AOIDE_STATE_DIR`; the two stage trees share
  `$AOIDE_STAGE_DIR` and otherwise fall back separately — `state/stage/`
  (conducting state) and `song/stage/` (rice staging). `~/Aoide` is the dev
  git checkout, reached via `$AOIDE_FLAKE_ROOT`, not a runtime path.
  Stage/state writes are atomic temp-then-rename.
- **Two registries, one convention:** `aoide schema --json` holds 81
  command paths (74 real, 7 stubs), `lyra schema --json` holds 48 (47 real,
  1 stub) — every group page prefixes
  each heading `aoide `/`lyra ` so the binary a command belongs to is never
  ambiguous.
- `secrets exec`'s parking is detailed in [[Secrets-Commands]].

## Group pages

- [[Rice-and-Livery|Rice-and-Livery]] — the self-ricing loop:
  `rice lint/stage/compose/declare/transpose`, the `rice draft` / `rice mode` /
  `rice take` groups, `rice back`, `cover set`, and the `livery` engine commands.
  Stage files: `song/stage/{livery,cover,mode}.json`; songbook and drafts trees.
- [[Graph-and-Conduct|Graph-and-Conduct]] — the session DAG: bare
  `graph`/`graph link` (the read/analysis lens), `project add/list/remove`,
  `session start/phase/end/hook/undying/permit/pending list/approve/deny/
  prune/reap`, bare `session` (the undying picker), bare `send`/`spawn`/
  `resurrect` (bare `resurrect` also walks up to a `.aoide/project.json`
  manifest), `conduct`, and `inbox list/read/clear` (the receive half of
  `send`).
  Stage files: `state/stage/{sessions,hooks,projects,graph,pending,
  herald}.json`, `state/inbox.json`; control sockets at
  `$XDG_RUNTIME_DIR/aoide/session-<id>.sock`.
- [[Screen-Commands|Screen-Commands]] — computer use: `screen info/
  shot/ocr/diff/send` and the nine `screen point` commands. Captures and sidecars
  in `state/captures/`; pointer position in `state/pointer-pos.json`.
- [[Doors-and-Nodes|Doors-and-Nodes]] — the other doors:
  `mcp serve`, `daemon`, `events tail`, `shellbridge`, `adapter melete`,
  `conductor` (signature + pointer to [[Conductor-TUI]]), `who`, `a2a serve`,
  and the `node` federation group (the outbound A2A client). Registries:
  `state/nodes.json`, `state/node-cache/<name>.json`.
- [[Conductor-TUI|Conductor-TUI]] — the `aoide conductor` interactive
  terminal frontend: seven panels, keys, what each dispatches. State:
  `state/stage/{projects,sessions,hooks}.json`, `song/stage/livery.json`,
  the audit log.
- [[Content-and-Hooks|Content-and-Hooks]] — the content
  pipeline commands (all stubs today), `herald push` (shellbridge socket →
  `state/stage/herald.json`), and `hooks install` (harness settings merge,
  `--capture` tee to `state/<agent>-hooks.jsonl`).
- [[Meta-and-Upkeep|Meta-and-Upkeep]] — `guide`, `schema`,
  `make`/`update` (stubs), `onboard` (the first-boot flow, core and lyra
  halves), `usage` (→ `state/usage.json`),
  `quickshell reload`/`healthcheck` (the live placeholder-screen watchdog),
  `soundcheck` (report-only sweep).
- [[Secrets-Commands|Secrets-Commands]] — the `aoide secrets` credential door: 17
  commands across direct-home admin, over-the-socket operator, and the daemon
  itself. State: `/run/aoide-secrets/{secrets.sock,events.jsonl}`, the
  secrets home.

## Related

- [[aoide-cli]]
- [[lyra]]
- [[Agent-Interface]]
- [[aoided]]
