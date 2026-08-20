# CLI Reference — aoide Command I/O Index

Dev-facing reference for the `aoide` command surface: for every command, what
it reads, what files it writes, and where its output pipes to. Grounded in the
Rust source (`pkgs/aoide/crates/`) and `aoide schema --json` — the pages state
the system at HEAD, not the design intent. For the concepts behind the verbs,
start at [[aoide-cli]] and the group pages linked below.

## Conventions that apply to every command

- **Exit codes:** `0` ok · `1` error · `2` usage · `64` not-implemented.
  A stub returns a structured `not-implemented` envelope, never a crash.
- **`--json` everywhere:** every command takes and emits a structured JSON
  envelope; without it, a human line goes to stdout.
- **Audit:** every dispatch — CLI, MCP, or A2A door — appends a JSON-lines
  record to `~/Aoide/log` (`$AOIDE_AUDIT_LOG` / `--audit-log` override).
- **Gating:** exactly three commands carry `gated: true` (`rice declare`,
  `content approve`, `update`) — the user rebuild gate ([[Rebuild-Gate]]).
  `graph send` and `screen send` hold pending approval internally instead.
- **Runtime roots:** repo-relative paths below resolve under `~/Aoide/` —
  `state/` via `$AOIDE_STATE_DIR`, `song/stage/` via `$AOIDE_STAGE_DIR`.
  Stage/state writes are atomic temp-then-rename.

## Group pages

- [[references/cli/Rice-and-Livery|Rice-and-Livery]] — the self-ricing loop:
  `rice lint/stage/compose/declare/transpose`, the `rice draft` / `rice mode` /
  `rice take` groups, `rice back`, `cover set`, and the `livery` engine verbs.
  Stage files: `song/stage/{livery,cover,mode}.json`; songbook and drafts trees.
- [[references/cli/Graph-and-Conduct|Graph-and-Conduct]] — the session DAG:
  `graph view/project/link/session/wrap/send/permit/focus/prune/reap/emit` and
  `conduct`. Stage files: `song/stage/{sessions,hooks,projects,graph,pending,
  herald}.json`; control sockets at `$XDG_RUNTIME_DIR/aoide/session-<id>.sock`.
- [[references/cli/Screen-Verbs|Screen-Verbs]] — computer use: `screen info/
  shot/ocr/diff/send` and the nine `screen point` verbs. Captures and sidecars
  in `state/captures/`; pointer position in `state/pointer-pos.json`.
- [[references/cli/Doors-and-Peers|Doors-and-Peers]] — the other doors:
  `mcp serve`, `daemon`, `shellbridge`, `adapter melete`, `conductor`, the
  `a2a` server + client group, and the `peer` federation group. Registries:
  `state/{a2a-agents,peers}.json`, `state/peer-cache/<name>.json`.
- [[references/cli/Content-and-Hooks|Content-and-Hooks]] — the content
  pipeline verbs (all stubs today), `herald push` (shellbridge socket →
  `song/stage/herald.json`), and `hooks install` (harness settings merge,
  `--capture` tee to `state/<agent>-hooks.jsonl`).
- [[references/cli/Meta-and-Upkeep|Meta-and-Upkeep]] — `guide`, `schema`,
  `make`/`update`/`onboard` (stubs), `usage` (→ `state/usage.json`),
  `quickshell reload`, `soundcheck` (report-only sweep).

## Related

- [[aoide-cli]]
- [[Agent-Interface]]
- [[aoided]]
