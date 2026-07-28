---
type: entity
created: 2026-07-26
updated: 2026-07-26
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

*Grounded in the repo at commit f3ceadf (walking-skeleton milestone), extended
at commits 0b3a3fd (the `graph` group), 41be90f (focus liveness, stage-dir
seam), and 8f4034e (the `shell dock toggle` keybind path). The crate lives at
`pkgs/aoide/`; it
packages via `rustPlatform.buildRustPackage` with `meta.mainProgram = "aoide"`
and vendored deps (`cargoLock.lockFile`) so the build is offline.*

## The command tree

`schema.rs` declares every command once, in a single `commands()` table — the
one source of truth from which the CLI dispatcher, the `schema --json` emitter,
and the MCP tool list all derive. Twenty-seven leaves — the nineteen
walking-skeleton commands plus the twelve-command `graph` group:

| Path | Real / stub | Gated |
|---|---|---|
| `guide` | real (prints tier-0 onboarding) | — |
| `schema` | real (emits the versioned schema doc) | — |
| `rice gen` | stub | — |
| `rice lint` | real (delegates to `drachma`) | — |
| `rice preview` | stub | — |
| `rice adopt` | stub | gated |
| `rice transpose` | stub | — |
| `content register` | stub | — |
| `content propose` | stub | — |
| `content approve` | stub | gated |
| `content ingest` | stub | — |
| `content query` | stub | — |
| `make` | stub (the [[Widget-Maker]] entry) | — |
| `update` | stub | gated |
| `onboard` | stub | — |
| `mcp serve` | real (stdio server) | — |
| `daemon` | real (runs the [[aoided]] skeleton) | — |
| `shellbridge` | real (runs the [[shellbridge]] process) | — |
| `adapter melete` | real (runs the [[Melete]] adapter) | — |
| `graph view` | real (Unicode DAG tree; `--focus`, `--json`) | — |
| `graph emit` | real (atomic `song/stage/graph.json` write) | — |
| `graph project add` | real (register a project, idempotent) | — |
| `graph project remove` | real (absent-ok) | — |
| `graph project list` | real | — |
| `graph link` | real (spawned-by edge; rejects self-links/cycles) | — |
| `graph session start` | real (UPSERT a running session; idempotent, startedAt-preserving, `--parent` cycle-checked) | — |
| `graph session phase` | real (upsert the one-per-session live hook phase) | — |
| `graph session end` | real (mark session + hook `done`; absent-ok) | — |
| `graph session hook` | real (stdin hook door; maps Claude-Code events; never exits non-zero) | — |
| `graph focus` | real (session jump via `hyprctl`, liveness-checked) | — |
| `graph prune` | real (drop `done` sessions, un-orphan children) | — |

The `graph` group (all real, none gated) is the [[Session-Graph]] viewer +
management layer — the project/session DAG that grows the
[[Terminal-Commander]] roster; the `guide` tier-1 blurb and the `docs/BUILD.md`
command table both carry it. `graph focus` no longer trusts `focuswindow`'s
always-zero exit: it confirms the window in `hyprctl clients -j` first
(case- and `0x`-tolerant address matching) and fails structured
(`window-not-found`, exit 1) for a vanished terminal; its five failure
reasons are `session-not-found` / `no-window-address` /
`hyprctl-unavailable` / `hyprctl-failed` / `window-not-found`. One schema gap
stands as an open thread: the compositor keybinds invoke `aoide shell
{launcher toggle, lock, dock toggle}`, a group **not** among the 27 leaves —
future CLI work. (`shell dock toggle` arrived at 8f4034e as the `SUPER+G`
open-and-pin path to the [[Gadget-Dock]] popup, replacing the earlier
`shell graph toggle` stub verb.) "Stub" elsewhere means a **structured not-implemented stub**: arg-parsing, the schema
entry, the audit-log append, and the gate flag are all real code paths; only the
live-system action is deferred. A stub returns an `Outcome` with status
`not-implemented` (exit 64), not a crash. The four `gated: true` commands (`rice
adopt`, `content approve`, `update`) are marked so both doors surface the user
rebuild gate uniformly — nothing here admits a rebuild, which is structurally the
user's action ([[Rebuild-Gate]], [[Governance]]).

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
  pinning the `song/stage/notes.json` format alongside the command tree so an
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
