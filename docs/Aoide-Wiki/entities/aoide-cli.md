---
type: entity
created: 2026-07-26
updated: 2026-08-14
aliases: [aoide binary, aoide command]
tags: [aoide, cli, agent, mcp, rust]
---

# aoide (the CLI binary)

The `aoide` binary is the complete capability surface of the framework — the
CLI trunk of [[Agent-Interface]], written in Rust as a single crate that ships
two binaries (`aoide` the CLI, `aoided` the daemon) plus a shared library.
"An API that happens to be typeable": every command emits structured `--json`,
every command carries meaningful exit codes, and the same handlers back the CLI
door, the MCP façade, and the [[A2A-Door]] so the three can never drift.

*The crate lives at `pkgs/aoide/`; it packages via `rustPlatform.buildRustPackage`
with `meta.mainProgram = "aoide"` and vendored deps (`cargoLock.lockFile`) so
the build is offline.*

## The command tree — a self-registering registry

`pkgs/aoide/src/registry.rs` declares the `Command`/`Registry` types (path,
summary, args, flags, `gated`, `implemented`, a `handler: fn(&Invocation) ->
Outcome`, an `available` check) plus the `cmd!`/`arg!`/`flag!` macros that
build one. Each command group under `pkgs/aoide/src/commands/`
(`meta.rs`, `rice.rs`, `draft.rs`, `mode.rs`, `cover.rs`, `stubs.rs`, `graph.rs`,
`infra.rs`, `a2a.rs`, `usage.rs`, `hooks.rs`) owns a
`register(&mut Registry)` function that inserts its own entries;
`commands/mod.rs::all()` assembles the full registry in the historical
`schema --json` order. Rice and cover handler bodies live in their own
command module; `commands/graph.rs` holds thin registrations whose
`handler` points straight at the `crate::graph::*` domain functions —
nothing there duplicates graph-domain logic.

`dispatch()` (`pkgs/aoide/src/dispatch.rs`) is a thin lookup against the one
process-wide `Registry` (built once via `OnceLock`) plus the audit-log
append and gate tail — the same shape run through by both doors. `schema
--json`, the MCP tool list, and the CLI's own path table all derive from
that one registry; there is no second, hand-maintained command table
anywhere in the crate. A `command_paths_match_the_golden_snapshot` unit
test in `registry.rs` pins the sorted set of every command path, so adding,
removing, or renaming a leaf shows as a deliberate, visible diff against
that pinned snapshot. `commands/mod.rs::all()`
builds the registry through explicit function calls, keeping the crate's
offline, vendored dependency set unchanged — the same shape as Hermes-agent's
self-registering tool registry and Claude Code's discrete-tools-behind-a-thin-
dispatch design.

The command surface itself is unchanged by this shape: **59 leaves**
(`aoide schema --json | jq '.commands | length'`):

| Group | Leaves | Real / stub |
|---|---|---|
| `guide`, `schema` | 2 | real |
| `rice lint`, `rice stage`, `rice compose` | 3 | real (`lint` runs the native [[livery]] engine) |
| `rice draft save/list/drop` | 3 | real |
| `livery emit`, `livery resolve`, `livery lint` | 3 | real — the engine's own verb group (the standalone note CLI's surface, native) |
| `rice mode status`/`stage`/`declarative`/`draft` | 4 | real |
| `cover set` | 1 | real |
| `rice declare`, `rice transpose` | 2 | stub (`declare` gated) |
| `content register/propose/ingest/query` | 4 | stub |
| `content approve` | 1 | stub, gated |
| `make` ([[Widget-Maker]] entry), `onboard` | 2 | stub |
| `update` | 1 | stub, gated |
| `mcp serve`, `daemon`, `shellbridge`, `adapter melete` | 4 | real |
| `graph` group (15 leaves — below) | 15 | real |
| `conduct` | 1 | real |
| `conductor` | 1 | real |
| `a2a serve`, `a2a agent add/list/remove/send` | 5 | real |
| `peer add/list/remove/pull/status` | 5 | real (same-network federation — `CONTRACTS.md` §7) |
| `usage` | 1 | real |
| `hooks install` | 1 | real |

The **`a2a`** group is the [[A2A-Door]] — the third door onto aoide. `a2a
serve` raises the A2A (Agent2Agent) JSON-RPC/HTTP server (a discoverable
AgentCard + `message/send` + `tasks/get` + SSE streaming, off by default,
loopback-bound); the four `a2a agent` verbs are the client side, registering
and driving external A2A agents through `state/a2a-agents.json`. **`usage`**
computes the local token/cost rollup that backs the opt-in claude.ai usage
gadget ([[Gadget-Dock]]). **`hooks install <agent> [--capture]`** is the
hook-installer verb: it merges aoide's hook wiring into the named harness's
settings file (path + format come from the harness's `AgentProfile` — claude:
JSON merge into `~/.claude/settings.json`; kimi: text-level `[[hooks]]`
append into `${KIMI_CODE_HOME:-~/.kimi-code}/config.toml`, never a
parse-rewrite since that file holds providers/credentials), idempotent and
reporting exactly which events were added vs already present. `--capture`
installs a parallel set of entries that tee raw payloads to
`~/Aoide/state/<agent>-hooks.jsonl` for debugging — a distinct idempotency
key, so capture entries coexist with the plain ones and are removed manually.
See [[Agent-Hooking]].

A stub returns a structured `Outcome` with status `not-implemented` (exit
`64`), never a crash — arg-parsing, the schema entry, the audit-log append,
and the gate flag are all real code paths regardless; only the live-system
action is deferred. Exactly three commands carry `gated: true` (`rice declare`,
`content approve`, `update`), marked so both doors surface the user rebuild
gate uniformly — nothing here admits a rebuild, which is structurally the
user's action ([[Rebuild-Gate]], [[Governance]]).

**`cover set`** stages `song/stage/cover.json` (the same atomic write-temp-
then-rename pattern as every other stage file) from either an absolute cover
path or a bare name resolved against `song/covers/`, hot-swapping the live
wallpaper. It is the CLI-verb slice of the wallpaper-switcher work; the
quickshell picker surface and a `list`/`next` verb pair remain unbuilt (open
item, `references/AOIDE-DEV.md` §7). Like `rice stage`, it refuses with a
`declarative-mode-locked` error while `rice mode declarative` is locked —
see [[Self-Ricing#Staging vs Declarative Mode]].

**`rice compose <name>`** (`--from <song>` · `--force` · `--json`)
scaffolds a new committed song — see [[Self-Ricing#Composing a song]] for
the shape it writes. It validates `name` and `--from` against a strict
`^[a-z0-9][a-z0-9-]*$` pattern (rejecting path traversal) and escapes
`$`/quotes when rendering a copied value into `rice.nix`, so a livery value
containing `${…}` can never round-trip into live Nix interpolation.

**`rice mode status`/`stage`/`declarative`/`draft`** is the three-way mode
toggle (`RiceMode`: `staging | declarative | draft`) around
`stage/mode.json` — `status` reports the current mode, `stage [<name>]`
unlocks `rice stage`/`cover set` and ALWAYS means plain declared content
(optionally staging a song in the same call; also leaves `draft` mode if
currently in it), `declarative [<name>]` re-pins `stage/livery.json` to the
resolved song's committed notes (given a name, or the current stage's own
song when none is given) and locks — discarding whatever unsaved live edits
sat in the stage, unless nothing was resolvable to re-pin from (also leaves
`draft` mode the same way `stage` does). `draft <name>` routes
`stage/livery.json` into `songbook/<song>/drafts/<name>/livery.json` via a
symlink (forking it from the current stage first if the name is new). See
[[Self-Ricing#Staging vs Declarative Mode]] and
[[Self-Ricing#Drafts — durable scratch, reached by ROUTING not copying]]
for the mechanism and why entrypoint guards, not a background reconciler,
are the enforcement.

**`rice draft save`/`list`/`drop`** manage saved drafts, nested under the
song they vary (`song/songbook/<song>/drafts/<name>/`) — never committed or
declared truth, gitignored, and banned from nix-eval reads. Reaching a
draft LIVE is `rice mode draft <name>`'s job (above), not this group's —
`save` is an independent, mode-agnostic fork of whatever's currently live
into a new/updated snapshot; `list [<song>]` enumerates saved drafts (all
songs, or one); `drop <name>` deletes one (missing name errors — not
idempotent-silent; refuses instead of dropping the currently-routed draft).
There used to be a fourth verb, `stage <name>` (copy-based) — superseded by
`rice mode draft`'s symlink routing and removed (no-internal-aliases rule:
keeping both would be two spellings of "go live with this draft"). See
[[Self-Ricing#Drafts — durable scratch, reached by ROUTING not copying]].

### The `graph` group — session/project DAG + the conductor mesh

`view`, `project add`/`remove`/`list`, `link`, `session start`/`phase`/`end`/
`hook` (takes `--agent <name>`, default `claude` — the payload maps through
that harness's agent profile, [[Agent-Hooking]]), `focus`, `prune`, `emit`
are the [[Session-Graph]] viewer +
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

### The `peer` group — aoide-to-aoide federation

`add <name> <url> [--autogate]` / `list` / `remove <name>` / `pull [<name>]`
/ `status` register OTHER aoide instances as **peers** and fold their
resolved session graphs into this instance's own — the newest door onto
aoide, built entirely on top of the existing [[A2A-Door]] rather than a new
transport (`aoide/graphSummary`, one new JSON-RPC method on the same
server). See [[Peer-Federation]] for the full mechanism (registry/cache
shapes, the `peer:*` graph-fold convention, and the non-loopback
pending-gate amendment `message/send` picked up alongside it). Same-network
only today — real, integration-tested
(`pkgs/aoide/crates/cli/tests/peer_connectivity.rs` proves two live
`a2a::serve()` instances talking peer-to-peer end to end) —
WAN/NAT-traversal reachability for a peer NOT on the same network is out of
scope for this v0.

- **`peer add`** — verifies the peer FIRST (fetches its AgentCard, mirroring
  `a2a agent add`'s verification-before-registering pattern) and only
  registers on success; a duplicate `name` is rejected rather than
  repointed, unlike `a2a agent add`'s upsert-on-readd.
- **`peer remove`** — a missing name is an error, not idempotent-silent
  (`rice draft drop`'s precedent, a deliberate divergence from `a2a agent
  remove`'s tolerate-missing stance); also drops that peer's cache file.
- **`peer pull`** — with no name, pulls EVERY registered peer; one peer
  being unreachable never aborts the others, and a failed pull marks the
  cache `stale` with a reason rather than deleting it, so a transient outage
  never blanks a peer out of the graph fold.
- **`peer status`** — each peer's `fresh`/`stale`/`never-pulled`
  classification (the same one the graph fold itself uses, so the two can
  never disagree) plus `fetchedAt`/`lastError`.

Registered directly after `a2a agent add/list/remove/send` in `schema
--json`'s order — nothing existing reorders.

### `conduct` and `conductor`

The two are deliberately distinct parts of speech. **`conduct`** is the verb —
`graph wrap`'s PTY-backed sibling — same register/wait/end lifecycle, but on a
controlling tty plus a per-session control socket, so `graph send` can type
into the running agent while its own TUI runs undisturbed. **`conductor`** is
the noun — the interactive terminal frontend over the whole trunk: a ratatui
TUI (DAG / sessions / projects / log / status panels, ~500ms poll, no
watcher/async runtime) that dispatches every action through the same
`dispatch()` the CLI and MCP doors use — never a second implementation, so the
one audit log can't tell a conductor keypress from a typed command.

### Open schema gap

The compositor keybind `SUPER+ESCAPE` (lock) still invokes `aoide shell
lock` — a verb group not among the leaves (open thread). `SUPER+G` (dock
toggle) is not part of this gap: like the launcher's `SUPER+SPACE` and the
wallpaper picker's `SUPER+W`, it triggers an in-process Hyprland global
shortcut the panel itself registers (`aoide:dock`), not a CLI verb (see
[[Quickshell]]).

## Contract-level conventions

- **One spelling per command, no internal aliases.** Every command has
  exactly one name; a retired spelling (e.g. the pre-rename `rice preview`/
  `rice mint`, or the once-aliased `rice new`) becomes a plain unknown
  command, resolved the same as a typo — never a parse-time alias to a
  canonical path. The parser carries no alias table.
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
  (`rice.lint`), args/flags become the `inputSchema`, and `tools/call` dispatches
  back into the same handlers the CLI uses.
- **`stageNotesVersion`** is a top-level field of the schema document (v0),
  pinning the `song/stage/livery.json` format alongside the command tree so an
  agent reads one version for the whole contract.
- **Single audit log, every door.** Every dispatch — CLI, MCP, or A2A —
  appends a JSON-lines record to the one audit log (`aoide.auditLog`), tagged
  with which door it came through (`Door::Cli` / `Door::Mcp` / `Door::A2a`). No
  door writes a separate log.

## How `rice lint` runs

`rice lint` is real: it runs the **native `livery::lint` engine** inside the
song crate (the native Rust validator, ported from the standalone Node
engine — same v0 schema, same error strings). No binary locate, no PATH shell-out: the engine is
compiled into `aoide` itself.

## The second binary — `aoided`

The same crate installs `aoided`, the daemon (see [[aoided]]). It shares the
audit-log, gate, and neutral-event-stream code with the CLI, so `aoide daemon`
and the standalone `aoided` binary reach the same code path; the standalone
binary is what the systemd unit launches. `aoided --audit-log <path>` overrides
the log location, else the `aoide.auditLog` default applies.

## Related

- [[Agent-Interface]]
- [[A2A-Door]]
- [[Peer-Federation]]
- [[Session-Graph]]
- [[Terminal-Commander]]
- [[Gadget-Dock]]
- [[livery]]
- [[aoided]]
- [[shellbridge]]
- [[Codebase]]
- [[Rebuild-Gate]]
- [[Governance]]
