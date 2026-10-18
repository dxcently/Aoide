---
type: concept
created: 2026-08-19
updated: 2026-10-10
tags: [aoide, cli, reference, schema]
---

# CLI Reference — aoide Command I/O Index

Dev-facing reference for the `aoide` and `strillix` command surfaces: for every command, what
it reads, what files it writes, and where its output pipes to. Grounded in the
Rust source (`pkgs/aoide/crates/`), `aoide schema --json`, and `strx schema
--json` — the pages state
the system at HEAD, not the design intent. For the concepts behind the commands,
start at [[aoide-cli]] and the group pages linked below.

## Conventions that apply to every command

- **Exit codes:** `0` ok · `1` error · `2` usage · `64` not-implemented.
  A stub returns a structured `not-implemented` envelope, never a crash.
- **Help screens:** bare `aoide`/`strillix`, `--help` and `help` print the overview
  (tagline, one line per head under its topic section, `(N commands)` for a
  group) on stdout at exit `0`; `aoide <group>` and `aoide <group> --help` list that group the same way. A mistake in the invocation
  exits `2` on stderr in the `[usage] … why: … fix: …` shape. Lines wrap to
  `COLUMNS` (else the tty width, else 80) with the text hanging under its own
  column. Color is the ANSI 16 base colors, on only for a tty and off for
  `NO_COLOR`, `TERM=dumb` and `--json`; `--color=auto|always|never` is global.
  The section order is declared once per binary (`commands::LAYOUT` in
  `pkgs/aoide/crates/cli` and `strillix`) and ships in `schema --json` as
  `sections`.
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
- **Runtime root:** the `state/`, `/stage/`, and `run/qml/` paths below
  hang off one root — `$AOIDE_ROOT` when set to an absolute path, default
  `<home>/.aoide` (`aoide_storage::fs::root`). Per-tree absolute overrides:
  `state/` via `$AOIDE_STATE_DIR`; the two stage trees share
  `$AOIDE_STAGE_DIR` and otherwise fall back separately — `state/stage/`
  (conducting state) and `/stage/` (rice staging). `~/Aoide` is the dev
  git checkout, reached via `$AOIDE_FLAKE_ROOT`, not a runtime path.
  Stage/state writes are atomic temp-then-rename.
- **Two registries, one convention:** `aoide schema --json` holds 81
  command paths (74 real, 7 stubs), `strx schema --json` holds 48 (47 real,
  1 stub) — every group page prefixes
  each heading `aoide `/`strillix ` so the binary a command belongs to is never
  ambiguous.
- `secrets exec`'s parking is detailed in [[Secrets-Commands]].

## Group pages

- [[Rice-and-Theme|Rice-and-Theme]] — the self-ricing loop:
  `rice lint/list/stage/compose/refresh/declare/transpose`, the `rice draft` / `rice mode` /
  `rice history` groups, `rice back`, `wallpaper set`, and the `theme` engine commands.
  Stage files: `/stage/{theme,wallpaper,mode}.json`, the declared twin
  `/declared/theme.json` and the venue's `/declared/venue.json` (the slots it recolours and the
  geometry the host sets);
  rices and drafts trees.
- [[Graph-and-Conduct|Graph-and-Conduct]] — the session DAG: bare
  `graph`/`graph link` (the read/analysis lens), `project add/list/remove`,
  `workspace set/clear/list/root` (the compositor workspace ↔ project binding;
  `root` prints one bare path for a launcher),
  `session start/phase/end/hook/grant/permit/pending list/approve/deny/
  prune/reap/find`, bare `session` (the roster — by project, or by host under
  `--hosts`), bare `send`/`spawn`/
  `resurrect` (bare `resurrect` also walks up to a `.aoide/project.json`
  manifest), `conduct`, and `mail`/`mail read`/`mail poll` (the receive half
  of `send`).
  Stage files: `state/stage/{sessions,hooks,projects,graph,pending,
  notifications}.json`; the mail store at `state/mail/` (one `boxes/<name>.log` per
  mailbox, `receipts.log`); control sockets at
  `$XDG_RUNTIME_DIR/aoide/session-<id>.sock`.
- [[Screen-Commands|Screen-Commands]] — computer use: `screen info/
  shot/ocr/diff/send` and the nine `screen point` commands. Captures and sidecars
  in `state/captures/`; pointer position in `state/pointer-pos.json`.
- [[Doors-and-Nodes|Doors-and-Nodes]] — the other doors:
  `mcp serve`, `daemon`, `events tail`, `shellbridge`, `adapter melete`,
  `conductor` (signature + pointer to [[Conductor-TUI]]), `a2a serve`,
  and the `node` federation group (the outbound A2A client). Registries:
  `state/nodes.json`, `state/node-cache/<name>.json`.
- [[Conductor-TUI|Conductor-TUI]] — the `aoide conductor` interactive
  terminal frontend: seven panels, keys, what each dispatches. State:
  `state/stage/{projects,sessions,hooks}.json`, `/stage/theme.json`,
  the audit log.
- [[Content-and-Hooks|Content-and-Hooks]] — the content
  pipeline commands (all stubs today), `notifications push` (shellbridge socket →
  `state/stage/notifications.json`), and `hooks install` (harness settings merge,
  `--capture` tee to `state/<agent>-hooks.jsonl`).
- [[Meta-and-Upkeep|Meta-and-Upkeep]] — `guide`, `schema`,
  `make`/`update` (stubs), `onboard` (the first-boot flow, core and strillix
  halves), `usage` (→ `state/usage.json`),
  `quickshell reload`/`healthcheck` (the live placeholder-screen watchdog),
  `soundcheck` (report-only sweep).
- [[Secrets-Commands|Secrets-Commands]] — the `aoide secrets` credential door: 17
  commands across direct-home admin, over-the-socket operator, and the daemon
  itself. State: `/run/aoide-secrets/{secrets.sock,events.jsonl}`, the
  secrets home.
- [[Apps-Commands|Apps-Commands]] — the `strx apps` group: `apps list/show/
  publish`, the installed desktop apps with icons resolved to files. State:
  `/stage/apps.json`.
- [[Clipboard-Commands|Clipboard-Commands]] — the `strx clipboard` group:
  `clipboard delete/clear`, one cliphist history entry or all of them, with
  the launcher's cached thumbnails.
- `strx lock` — the one lock entry (`SUPER+ESCAPE`, the power menu, the `lock`
  alias): the running shell's `LockHost` in the staged rice's face when that
  rice dresses the `lockscreen` slot and the shell's PAM service
  (`/etc/pam.d/aoide-lock`) exists, confirmed by the compositor within 5 s,
  else hyprlock, which clears a `LockedHint` a dead shell lock left at `yes`;
  `--hyprlock` skips the shell (the lock host's hand-over when its PAM
  conversation fails while it holds the lock).
  `strx lock hint yes|no` sets logind's `LockedHint`, which the lock host
  calls on lock and unlock; bare `strx lock hint` reads it, which the lock
  host does at start to take back a lock its crashed predecessor held.

## Related

- [[aoide-cli]]
- [[strillix]]
- [[Agent-Interface]]
- [[aoided]]
