---
type: entity
created: 2026-07-26
updated: 2026-08-25
aliases: [aoide binary, aoide command]
tags: [aoide, cli, agent, mcp, rust]
---

# aoide (the CLI binary)

`aoide` is Aoide's core orchestration surface: conducting, the project/
session graph, A2A, peer federation, the daemon, usage, and hooks. It is the
CLI trunk of [[Agent-Interface]]. The crate ships two binaries, `aoide` (CLI)
and `aoided` (daemon), from a pi-style workspace of single-charter crates
(`protocol`, `storage`, `client`, `conduct`, `server`, `conductor`, `upkeep`,
`cli` — [[Package-Layout]]). Every command emits structured `--json` and
carries a meaningful exit code. The CLI, the MCP façade, and the
[[A2A-Door]] share the same handlers, so the three doors cannot drift.

Everything that paints — rice/draft/mode/cover/livery/quickshell/screen/
shellbridge/herald/take, the whole AoideOS surface — ships in a second
binary, [[lyra]]. Conducting orchestration is aoide's identity; lyra paints
(`docs/architecture/PACKAGE-LAYOUT.md` "Two binaries"; `CONTRACTS.md` §3).
See [[Rice-and-Livery|Rice-and-Livery]] and
[[Screen-Commands|Screen-Commands]] for that surface's per-command
detail.

*The crate root lives at `pkgs/aoide/crates/cli/`; it packages via
`rustPlatform.buildRustPackage` with `meta.mainProgram = "aoide"` and vendored
deps (`cargoLock.lockFile`) so the build is offline. Core is nix-independent:
cargo-buildable on any Linux, zero nix shell-outs — only `lyra` may shell out
to nix.*

## The command tree — a self-registering registry

`pkgs/aoide/crates/cli/src/registry.rs` declares the `Command`/`Registry`
types (path, summary, args, flags, `gated`, `implemented`, a handler
function, an `available` check) and the `cmd!`/`arg!`/`flag!` macros that
build them, shared via `aoide-protocol`. Each domain crate registers its own
entries through its own `commands::register(&mut Registry)` function;
`cli/src/commands/mod.rs::all()` assembles the full registry in `schema
--json`'s order, calling into `aoide_server`, `aoide_conduct`,
`aoide_client`, `aoide_conductor`, `aoide_storage`, `aoide_upkeep`, and
`aoide_secrets` in turn, plus its own `meta`/`stubs`/`infra` groups.
`aoide_secrets::commands::register` is newest, contributing the
[[Secrets-Broker]] group (`serve`/`exec`/`add`/`rm`/`grant`/`revoke`/
`enroll`/`put`/`set-totp`/`automate`/`expose`/`migrate`/`pending`/`approve`/
`dismiss`/`watch`). P-A5 of the binary-split workstream moved 11 painted
groups (rice/draft/mode/cover/livery/shellbridge/quickshell/screen/herald/
take — 39 command paths) into `crates/lyra/src/commands/mod.rs::all()`.

`dispatch()` looks up one process-wide `Registry` (built once via
`OnceLock`), appends to the audit log, and applies the gate — the same path
run through by both doors. `schema --json`, the MCP tool list, and the
CLI's own path table all derive from that one registry; there is no second,
hand-maintained command table. A `command_paths_match_the_golden_snapshot`
unit test in `registry.rs` pins the sorted set of every command path, so
adding, removing, or renaming a leaf shows as a deliberate diff against that
snapshot.

The command surface holds **65 leaves across the groups this page tracks**;
`aoide schema --json | jq '.commands | length'` reports 80, since further
commands exist that are not yet covered here: the `inbox` group, `who`,
`events tail`, `identity`, and the `peer` group's `hub`/`allow`/`spawn`,
`discover`/`invite`, and 4-command `pair` ceremony. `lyra schema --json`
carries the painted surface — see above.
The per-command dev reference — signature, files read, files written,
where output pipes to — lives at [[CLI-Reference]]; the table below sums
the groups it documents.

| Group | Leaves | Real / stub |
|---|---|---|
| `guide`, `schema` | 2 | real |
| `content register/propose/ingest/query` | 4 | stub |
| `content approve` | 1 | stub, gated |
| `make` ([[Widget-Maker]] entry), `onboard` | 2 | stub |
| `update` | 1 | stub, gated |
| `mcp serve`, `daemon` | 2 | real |
| `graph` group (incl. `pending list/approve/deny`, `spawn` — below) | 21 | real |
| `conduct` | 1 | real |
| `adapter melete` | 1 | real |
| `conductor` | 1 | real |
| `a2a serve`, `a2a agent add/list/remove/send` | 5 | real |
| `peer add/list/remove/pull/status` | 5 | real (same-network federation — `CONTRACTS.md` §7) |
| `secrets serve/exec/add/rm/grant/revoke/enroll/put/set-totp/automate/expose/migrate/pending/approve/dismiss/watch` | 16 | real ([[Secrets-Broker]] — TOTP-gated resolves, socket-only, own uid) |
| `usage` | 1 | real |
| `hooks install` | 1 | real |
| `soundcheck` | 1 | real — report-only mechanical-integrity sweep of the working tree; writes nothing |

The **`a2a`** group is the [[A2A-Door]], the third door onto aoide. `a2a
serve` raises the A2A JSON-RPC/HTTP server (AgentCard, `message/send`,
`tasks/get`, SSE streaming; off by default, loopback-bound). Its
`--bearer-secret <name>` names a secret this door requires as the inbound
`Authorization: Bearer` token, resolved fresh per request through
[[Secrets-Broker]] and failing closed on a resolve error. The four `a2a
agent` commands register and drive external A2A agents through
`state/a2a-agents.json`.

**`usage`** computes the local token/cost rollup behind the opt-in claude.ai
usage gadget ([[Gadget-Dock]]).

**`secrets`** is the credential door: `secrets exec`/`secrets put` release a
value into the caller's own process without logging it, gated by per-secret
`consumers[]` and an optional TOTP code. `secrets pending`/`approve`/
`dismiss` complete or refuse a codeless TOTP resolve that parked instead of
refusing outright. `secrets watch [--popup]` is the terminal (or zenity)
surface that completes a parked ask live. See [[Secrets-Broker]].

**`hooks install <agent> [--capture]`** merges aoide's hook wiring into the
named harness's settings file — path and format come from the harness's
`AgentProfile` (claude: JSON merge into `~/.claude/settings.json`; kimi:
text-level `[[hooks]]` append into `${KIMI_CODE_HOME:-~/.kimi-code}/
config.toml`, never a parse-rewrite since that file also holds providers/
credentials). It is idempotent and reports exactly which events were added
versus already present. `--capture` installs a parallel set of entries that
tee raw payloads to `~/Aoide/state/<agent>-hooks.jsonl` for debugging, under
a distinct idempotency key so capture entries coexist with the plain ones
(removed manually). See [[Agent-Hooking]].

A stub returns a structured `Outcome` with status `not-implemented` (exit
`64`), never a crash — arg-parsing, the schema entry, the audit-log append,
and the gate flag are all real code paths; only the live-system action is
deferred. Exactly two commands carry `gated: true` in `aoide` (`content
approve`, `update`); a third, `rice declare`, is `lyra`'s own gated stub —
both doors surface the user rebuild gate uniformly, and nothing here admits
a rebuild ([[Rebuild-Gate]], [[Governance]]).

`cover set`/`rice compose`/`rice mode`/`rice draft`/`rice declare`/`rice
transpose`/`rice take`/`rice back` and the `livery`/`screen`/`herald`/
`shellbridge`/`quickshell` groups all moved to `lyra` at P-A5 — see
[[Rice-and-Livery|Rice-and-Livery]] and
[[Screen-Commands|Screen-Commands]] for their per-command
reference, and [[Self-Ricing]] for the self-ricing loop's walkthrough.

### The `graph` group — session/project DAG + the conductor mesh

`view`, `project add`/`remove`/`list`, `link`, `session start`/`phase`/
`end`/`hook` (takes `--agent <name>`, default `claude`; the payload maps
through that harness's agent profile, [[Agent-Hooking]]), `permit`,
`focus`, `prune`, `emit` are the [[Session-Graph]] viewer and manager
feeding the [[Terminal-Commander]] roster (see [[Agent-Hooking]] for the
session-registration doors). `graph permit --id <id>` answers a harness
permission prompt inside a conducted session by typing the profile's
verified permission key (claude's `1`/`3`). Three commands turn the graph
into a live conductor mesh:

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

`add <name> <url> [--autogate] [--token-file <path>] [--bearer-secret
<name>]` / `list` / `remove <name>` / `pull [<name>]` / `status` register
OTHER aoide instances as **peers** and fold their resolved session graphs
into this instance's own — built entirely on top of the existing
[[A2A-Door]] rather than a new transport (`aoide/graphSummary`, one new
JSON-RPC method on the same server). See [[Peer-Federation]] for the full
mechanism. Same-network only today — real, integration-tested
(`pkgs/aoide/crates/cli/tests/peer_connectivity.rs` proves two live
`a2a::serve()` instances talking peer-to-peer end to end); WAN/NAT-traversal
reachability is out of scope for this v0.

- **`peer add`** — verifies the peer FIRST (fetches its AgentCard, mirroring
  `a2a agent add`) and only registers on success; a duplicate `name` is
  rejected rather than repointed, unlike `a2a agent add`'s upsert-on-readd.
  `--token-file` records a per-peer bearer secret this instance expects that
  peer to present, identifying WHICH peer is calling once address alone
  can't (a proxy or tunnel makes every caller's address look loopback).
  `--bearer-secret` is the outbound counterpart: it names a secret,
  resolved fresh through [[Secrets-Broker]], this instance presents as its
  own `Authorization: Bearer` header when calling that peer.
- **`peer remove`** — a missing name is an error, not idempotent-silent
  (`rice draft drop`'s precedent); also drops that peer's cache file.
- **`peer pull`** — with no name, pulls EVERY registered peer; one peer
  being unreachable never aborts the others, and a failed pull marks the
  cache `stale` with a reason rather than deleting it.
- **`peer status`** — each peer's `fresh`/`stale`/`never-pulled`
  classification (the same one the graph fold itself uses) plus
  `fetchedAt`/`lastError`.

Registered directly after `a2a agent add/list/remove/send` in `schema
--json`'s order.

### `conduct` and `conductor`

The two are deliberately distinct parts of speech. **`conduct`** is the
command — `graph wrap`'s PTY-backed sibling, same register/wait/end lifecycle,
but on a controlling tty plus a per-session control socket, so `graph send`
can type into the running agent while its own TUI runs undisturbed.
**`conductor`** is the noun — the interactive terminal frontend over the
whole trunk: a ratatui TUI (DAG / sessions / projects / log / status
panels, ~500ms poll, no watcher/async runtime) that dispatches every action
through the same `dispatch()` the CLI and MCP doors use.

### Open schema gap

The compositor keybind `SUPER+ESCAPE` (lock) still invokes `aoide shell
lock` — a command group not among the leaves (open thread). `SUPER+G` (dock
toggle) is not part of this gap: like the launcher's `SUPER+SPACE` and the
wallpaper picker's `SUPER+W`, it triggers an in-process Hyprland global
shortcut the panel itself registers (`aoide:dock`), not a CLI command (see
[[Quickshell]]).

## Contract-level conventions

- **One spelling per command, no internal aliases.** Every command has
  exactly one name; a retired spelling (e.g. the pre-rename `rice preview`/
  `rice mint`) becomes a plain unknown command, resolved the same as a typo.
  The parser carries no alias table.
- **`--json` everywhere.** A hand-rolled parser (no clap, to keep the
  offline cargo lock tiny) reads argv against the schema, so the parser and
  the schema can never disagree about what commands exist.
- **Exit-code map, identical per command:** `0` ok · `1` error · `2` usage ·
  `64` not-implemented. Serialized into both `schema --json` and every
  `Outcome`.
- **`schema --json` is the single source of truth.** It emits the raw
  contract document at top level (`schemaVersion`, `aoide` version,
  `stageNotesVersion`, and the command array) — not wrapped in the generic
  outcome envelope, so external tooling and the MCP tool list parse it
  directly. The MCP door (`mcp serve --stdio`) is a minimal
  dependency-free JSON-RPC 2.0 server over newline-delimited stdio: each
  command becomes one tool named by its dotted path (`graph.view`),
  args/flags become the `inputSchema`, and `tools/call` dispatches back
  into the same handlers the CLI uses. `lyra mcp serve --stdio` is the same
  façade over lyra's own registry.
- **`stageNotesVersion`** pins the `song/stage/livery.json` format alongside
  the command tree so an agent reads one version for the whole contract.
- **Single audit log, every door.** Every dispatch — CLI, MCP, or A2A —
  appends a JSON-lines record to the one audit log (`aoide.auditLog`),
  tagged with which door it came through (`Door::Cli` / `Door::Mcp` /
  `Door::A2a`).

## The second binary — `aoided`

The same crate installs `aoided`, the daemon (see [[aoided]]). It shares
the audit-log, gate, and neutral-event-stream code with the CLI, so `aoide
daemon` and the standalone `aoided` binary reach the same code path; the
standalone binary is what the systemd unit launches. `aoided --audit-log
<path>` overrides the log location, else the `aoide.auditLog` default
applies.

## Related

- [[Agent-Interface]]
- [[A2A-Door]]
- [[Peer-Federation]]
- [[Secrets-Broker]]
- [[Screen-Control]]
- [[Session-Graph]]
- [[Terminal-Commander]]
- [[Gadget-Dock]]
- [[livery]]
- [[lyra]]
- [[aoided]]
- [[shellbridge]]
- [[Codebase]]
- [[Rebuild-Gate]]
- [[Governance]]
