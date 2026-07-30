---
type: entity
created: 2026-07-26
updated: 2026-07-30
aliases: [aoide binary, aoide command]
tags: [aoide, cli, agent, mcp, rust]
---

# aoide (the CLI binary)

The `aoide` binary is the complete capability surface of the framework — the
CLI trunk of [[Agent-Interface]], written in Rust as a single crate that ships
two binaries (`aoide` the CLI, `aoided` the daemon) plus a shared library.
"An API that happens to be typeable": every command emits structured `--json`,
every command carries meaningful exit codes, and the same handlers back both
the CLI door and the MCP door so the two can never drift.

*The crate lives at `pkgs/aoide/`; it packages via `rustPlatform.buildRustPackage`
with `meta.mainProgram = "aoide"` and vendored deps (`cargoLock.lockFile`) so
the build is offline.*

## The command tree

`schema.rs` declares every command once, in a single `commands()` table — the
one source of truth from which the CLI dispatcher, the `schema --json` emitter,
and the MCP tool list all derive. **37 leaves** (`aoide schema --json | jq
'.commands | length'`):

| Group | Leaves | Real / stub |
|---|---|---|
| `guide`, `schema` | 2 | real |
| `rice lint`, `rice preview` | 2 | real (`lint` delegates to [[drachma]]) |
| `cover set` | 1 | real |
| `rice gen`, `rice adopt`, `rice transpose` | 3 | stub (`adopt` gated) |
| `content register/propose/ingest/query` | 4 | stub |
| `content approve` | 1 | stub, gated |
| `make` ([[Widget-Maker]] entry), `onboard` | 2 | stub |
| `update` | 1 | stub, gated |
| `mcp serve`, `daemon`, `shellbridge`, `adapter melete` | 4 | real |
| `graph` group (15 leaves — below) | 15 | real |
| `conduct` | 1 | real |
| `baton` | 1 | real |

A stub returns a structured `Outcome` with status `not-implemented` (exit
`64`), never a crash — arg-parsing, the schema entry, the audit-log append,
and the gate flag are all real code paths regardless; only the live-system
action is deferred. Exactly three commands carry `gated: true` (`rice adopt`,
`content approve`, `update`), marked so both doors surface the user rebuild
gate uniformly — nothing here admits a rebuild, which is structurally the
user's action ([[Rebuild-Gate]], [[Governance]]).

**`cover set`** stages `song/stage/cover.json` (the same atomic write-temp-
then-rename pattern as every other stage file) from either an absolute cover
path or a bare name resolved against `song/covers/`, hot-swapping the live
wallpaper. It is the CLI-verb slice of the wallpaper-switcher work; the
quickshell picker surface and a `list`/`next` verb pair remain unbuilt (open
item, `references/AOIDE-DEV-HANDOFF.md` §7).

### The `graph` group — session/project DAG + the conductor mesh

`view`, `project add`/`remove`/`list`, `link`, `session start`/`phase`/`end`/
`hook`, `focus`, `prune`, `emit` are the [[Session-Graph]] viewer +
manager feeding the [[Terminal-Commander]] roster (see [[Agent-Hooking]] for
the session-registration doors). Three commands turn the graph into a
live conductor mesh:

- **`graph wrap`** — spawn any agent command as a registered session
  (inherited stdio, `running`→`done` for free, `AOIDE_SESSION_ID` exported to
  the child). See [[Agent-Hooking]].
- **`graph reap`** — the liveness sweeper: marks a session `done` when its
  window is gone (`hyprctl clients -j`) or its pid's `/proc` entry is gone,
  run on a systemd user timer (~12s) as the companion to `graph prune`
  (which only drops sessions already `done`).
- **`graph send`** — the one gated injection door: types text into a
  *conducted* session's control socket. Held pending approval by default;
  `--yes` (or an autogate policy) delivers and renames the node to a
  one-line form of the text. Every outcome is audited.

`graph focus` distrusts `focuswindow`'s always-zero exit: it confirms the
window in `hyprctl clients -j` first (case- and `0x`-tolerant address
matching) and fails structured (`window-not-found`, exit 1) for a vanished
terminal; its five failure reasons are `session-not-found` /
`no-window-address` / `hyprctl-unavailable` / `hyprctl-failed` /
`window-not-found`.

### `conduct` and `baton`

**`conduct`** is `graph wrap`'s PTY-backed sibling — same
register/wait/end lifecycle, but on a controlling tty plus a per-session
control socket, so `graph send` can type into the running agent while its own
TUI runs undisturbed. **`baton`** is the interactive terminal frontend over
the whole trunk: a ratatui TUI (DAG / sessions / projects / log / status
panels, ~500ms poll, no watcher/async runtime) that dispatches every action
through the same `dispatch()` the CLI and MCP doors use — never a second
implementation, so the one audit log can't tell a baton keypress from a typed
command.

### Open schema gap

The compositor keybind `SUPER+ESCAPE` (lock) still invokes `aoide shell
lock` — a verb group not among the leaves (open thread). `SUPER+G` (dock
toggle) is not part of this gap: like the launcher's `SUPER+SPACE` and the
wallpaper picker's `SUPER+W`, it triggers an in-process Hyprland global
shortcut the panel itself registers (`aoide:dock`), not a CLI verb (see
[[Quickshell]]).

## Contract-level conventions

- **`--json` everywhere.** A hand-rolled parser (no clap, to keep the offline
  cargo lock tiny) reads argv against the schema, so the parser and the schema
  can never disagree about what commands exist. Every outcome renders as either
  a human line or a pretty-printed JSON envelope.
- **Exit-code map, identical per command:** `0` ok · `1` error · `2` usage · `64`
  not-implemented. Serialized into both `schema --json` and every `Outcome`.
- **`schema --json` is the single source of truth.** It emits the raw contract
  document at top level (schema version, `aoide` version, `stageNotesVersion`,
  and the command array) — *not* wrapped in the generic outcome envelope, because
  external tooling and the MCP tool list parse it directly. The MCP door
  (`mcp serve --stdio`) is a minimal dependency-free JSON-RPC 2.0 server over
  newline-delimited stdio: each command becomes one tool named by its dotted path
  (`rice.gen`), args/flags become the `inputSchema`, and `tools/call` dispatches
  back into the same handlers the CLI uses.
- **`stageNotesVersion`** is a top-level field of the schema document (v0),
  pinning the `song/stage/drachma.json` format alongside the command tree so an
  agent reads one version for the whole contract.
- **Single audit log, both doors.** Every dispatch — CLI or MCP — appends a
  JSON-lines record to the one audit log (`aoide.auditLog`), tagged with which
  door it came through. Neither door writes a separate log.

## How it finds `drachma`

`rice lint` is real: it shells out to [[drachma]] `lint`. It locates that
binary in order — explicit `$AOIDE_DRACHMA_BIN`, then a PATH lookup for
`drachma`.
Absence is tolerated with a structured error (`notes-binary-unavailable`), never
a panic — the walking-skeleton contract.

## The second binary — `aoided`

The same crate installs `aoided`, the daemon (see [[aoided]]). It shares the
audit-log, gate, and neutral-event-stream code with the CLI, so `aoide daemon`
and the standalone `aoided` binary reach the same code path; the standalone
binary is what the systemd unit launches. `aoided --audit-log <path>` overrides
the log location, else the `aoide.auditLog` default applies.

## Related

- [[Agent-Interface]]
- [[Session-Graph]]
- [[Terminal-Commander]]
- [[Gadget-Dock]]
- [[drachma]]
- [[aoided]]
- [[shellbridge]]
- [[Codebase]]
- [[Rebuild-Gate]]
- [[Governance]]
