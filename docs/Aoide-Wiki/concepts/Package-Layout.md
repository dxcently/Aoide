---
type: concept
created: 2026-08-01
updated: 2026-08-27
tags: [aoide, architecture, rust, cli, crate, blueprint]
---

# Package Layout — the pi-Style Crate Split

**Status: both splits are complete.** `pkgs/aoide` is a virtual Cargo
workspace of **13 crates** — `protocol`, `storage`, `conduct`, `client`,
`server`, `song`, `conductor`, `cli`, `test-support`, `upkeep`, `lyra`,
`screen`, `secrets` — assembled into **two binaries**: `aoide`/`aoided`
(core) and `lyra` (paint). This is the
[pi](https://github.com/earendil-works/pi)-style split; grounded in the
repo's `docs/architecture/PACKAGE-LAYOUT.md`, which this page mirrors in the
wiki's own voice and trims for length — read it for the full phase-by-phase
history this page only summarizes. `steward`, `management`, and `evals` are
specified, unbuilt crates — see their own Status notes below.

## Why split at all

pi's monorepo is the reference for a discipline a single crate does not have:
**one package, one charter, one dependency boundary**, wired together by a
thin app package at the top. The non-negotiable invariant the split
preserves is **"one schema, N doors"** ([[Agent-Interface]]: CLI trunk + MCP
façade + [[A2A-Door]] all deriving from one registry, never drifting) — in
the split, that single source is its own crate (`protocol`) every door
depends on, making the invariant structural rather than conventional.

The split also carries a distribution goal: **Aoide-the-core ships as its
own package, runnable on any Linux distribution through Nix** (the package
manager, not the OS) — NixOS is one target (AoideOS), never a requirement.
Core (`aoide`/`aoided`) has no dependency on this repo's NixOS modules; only
`lyra` may shell out to nix (`song::widgets`'s `nix eval` call for the
songbook manifest).

## The pi → aoide mapping

| pi package | aoide crate | why it differs |
|---|---|---|
| `protocol` (cbor/framing/schemas) | **`protocol`** | the registry-derived schema + `Outcome` envelope + door types is aoide's wire, not a CBOR frame codec — same role: the contract every surface reads |
| `ai` (multi-provider LLM API) | *(dropped)* | agents bring their own shell + model; aoide wraps no LLM API |
| `agent` (loop + harness/tools/session/memory) | **`steward`** (unbuilt) | a system-management agent, not a coding agent — see Status below |
| `client` (connection/transport/session) | **`client`** | outbound: the A2A client + the melete adapter + transports that drive external agents and talk to `aoided` |
| `server` (listener/sessions/snapshots) | **`server`** | `aoided`: the door serve-loops, listeners, sessions, snapshots, audit sink |
| `storage`/`sqlite-node` | **`storage`** | durable session data + memory persistence; file-first today |
| `evals` (vitest-eval harness) | **`evals`** (unbuilt) | see Status below |
| `tui` (terminal UI library) | **`conductor`** | the session DAG view + roster CLI/TUI |
| `coding-agent` (the concrete app) | **`cli`** / **`lyra`** | the two app crates that assemble every domain crate into the shipped binaries |
| *(none)* | **`conduct`** | aoide-unique: the PTY multiplexer + session DAG + hook plumbing that makes every terminal a tracked, conductable session |
| *(none)* | **`song`** | aoide-unique: the ricing/design engine (notes → livery, songs, stage) |
| *(none)* | **`screen`** | aoide-unique: read-only desktop view + pointer synthesis for agents |
| *(none)* | **`secrets`** | aoide-unique: the socket-only credential broker |
| *(none)* | **`management`** (unbuilt) | the privileged hands — see Status below |

## The 13 landed crates

| crate | charter | app | commands it registers |
|---|---|---|---|
| **protocol** | The contract: registry, schema doc, `Outcome` envelope + exit codes, `Door`, audit event classes, A2A-JSON-RPC/MCP wire types, the `cmd!`/`arg!`/`flag!` registration macros. Every door depends on it; it depends on nothing aoide-specific. | both | none (library) |
| **conduct** | The session core: PTY-backed `conduct` wrap, the session DAG, hook ingestion, liveness reaping. Also owns the `shellbridge`/`herald` implementation files (a deliberate charter exception — both are entangled with core internals even though their CLI commands register into `lyra`). | both | aoide: `conduct`, `graph.*` (20), `hooks.install`, `who`; lyra: `shellbridge`, `herald.push` |
| **server** | `aoided` and the door serve-loops: MCP-over-stdio, A2A JSON-RPC/HTTP/SSE, listeners, sessions, snapshots, the audit sink. Untrusted input stops here. | aoide | `daemon`, `a2a.serve` |
| **client** | Outbound: the A2A client registry + send, the melete adapter, transports that drive external agents and speak to `aoided`. | aoide | `adapter.melete`, `a2a.agent.*` (4), `peer.*` (5) |
| **storage** | Durable session data + memory persistence: session store, transcript index, stage/state files, migrations. File-first (seed → build; a real embedded store is the open question below). | aoide | `usage`, `inbox.*` (3) |
| **secrets** | The credential broker: a socket-only daemon under its own uid, TOTP-gated parked resolves, `file`/`age` backends, `secrets watch`/`--popup`. See [[Secrets-Broker]]. | aoide | `secrets.*` (16) |
| **conductor** | The session-DAG TUI (ratatui: app/ui/graphview/theme). Depends on `{protocol, conduct, storage}` via a dependency-injection seam (`App` takes a `DispatchFn`), never on `server`. | aoide | `conductor` |
| **upkeep** | Mechanical integrity, the working-tree half: `soundcheck` polices gitignored/uncommitted state (`result`, `state/`, stray root files) that `nix flake check` structurally cannot see. Report-only, forever — never moves, deletes, formats, or repairs. | aoide | `soundcheck` |
| **cli** | The app shell: `aoide`/`aoided` bins, arg parse, the single dispatcher, registry assembly (`commands::all()`), guide/onboarding, and the three root-coupled groups (`meta`, `stubs`, `mcp serve`) that read the fully-assembled registry. Depends on every core domain crate; nothing depends on it. | aoide | `guide`, `schema`, `mcp.serve`, `content.*` (5, stub), `make`/`update` (stub), `onboard` |
| **song** | The ricing/design engine: the native livery engine (schema · resolve · emit), rice/draft/mode/cover/take, the Pantheon design language. Rices portably — no Stylix/NixOS-module dependency in the crate itself. | lyra | `rice.*` (3), `rice.draft.*` (3), `rice.mode.*` (4), `cover.set`, `livery.*` (3), `rice.declare`/`.transpose` (stub), `quickshell.reload`, `rice.take.*` (6) |
| **screen** | A read-only view onto the desktop for agents plus pointer synthesis: `info`/`shot`/`ocr`/`diff`/`send`, `point` (native `zwlr_virtual_pointer_v1` synthesis, 9 subverbs). Carved out (P-A1) specifically to isolate wayland/image dependency weight off core. | lyra | `screen.*` (14) |
| **test-support** | Shared test rig only — scratch dirs, `EnvSaver`, fixture payloads, the single `env_lock`. A dev-dependency, never a production edge. | both (dev-only) | none |
| **lyra** | The second app shell: arg parse, dispatcher, registry assembly for the paint bundle. The one binary allowed to shell out to nix. Never registers `a2a serve`, `conductor`, or any core-only group. | lyra | `guide`, `schema`, `mcp.serve`, `onboard` |

Command totals: **aoide 81** (74 real, 7 stub), **lyra 43** (41 real, 2 stub)
— see [[Full-Architecture]] for the full per-group breakdown.

## Two binaries — `aoide`/`aoided` (core) and `lyra` (paint)

Core keeps `aoide`/`aoided` over `protocol` · `storage` · `client` ·
`conduct` · `server` · `conductor` · `upkeep` · `cli`. `lyra` (crate
`aoide-lyra`) owns the rice/draft/mode/cover/livery/quickshell/screen/
shellbridge/herald command surface — everything that paints, or that only a
desktop needs.

- **Naming.** Muse names (`Aoide`, `Melete`, `Mneme`) name systems, not
  binaries; a binary living inside a system takes an instrument name — the
  desktop is the instrument the muse plays.
- **`conductor` stays core.** It is the messaging/orchestration surface a
  headless box needs most, not a painting tool: pure Rust (ratatui), no
  system-closure weight, and conducting orchestration is Aoide's core
  identity. It ships in `aoide`/`aoided`, never `lyra`.
- **Charter exceptions, named as the smudges they are.** `shellbridge.rs`
  and `herald.rs` stay as files in `conduct` — only their registry lines (the
  CLI commands) move to `lyra` — because both are entangled with core:
  `session permit` publishes summons through `herald`, and the conductor TUI
  reads the socket path `shellbridge` owns. `storage::takes` and
  `storage::mode` stay in `storage` for the same reason: zero dependency
  weight, and `mode` is read by `shellbridge`, itself core-crate-resident.
- **`management` is untouched by this split** — still deferred (see below),
  waiting on real host-ops commands to exist before there is anything to
  extract.
- **Nix-independence.** Core builds with `cargo` and runs on any Linux — no
  nix shell-outs, no NixOS assumption. This is the structural half of the
  distribution goal above.

## Deferred crates

- **`steward`** — a system-management agent driven by the `conductor` TUI:
  a harness loop whose tools act on the host via `management`/`conduct`/
  `song`, remembering design primitives in `canon`, self-auditing against
  `canon` + CONTRACTS. **Status: specified, not built.** Two of its five
  pillars are blocked: `harness` would require aoide to adopt an LLM client
  it has explicitly refused to own ("any agent with a shell is fully
  capable" is the core thesis); `tools`/`skills` need `management`'s
  host-ops commands, which don't exist. The design-decision corpus `canon`
  would formalize (`song/songbook/*/design/*.md`, this wiki's design pages)
  is prose today, not structured records — nothing to wrap a crate around
  yet.
- **`management`** — the privileged hands: rebuild/switch, `rice mint`, hypr
  control, service/daemon ops, sudo-tracking, host-abstracted behind a NixOS
  backend and a portable-Nix backend. **Status: specified, not built, no
  ETA.** No `nixos-rebuild`/`switch`/`systemctl` code exists anywhere in the
  Rust workspace — deliberate house policy (rebuild is user-gated, no
  self-updaters). The one real host-effect in the ricing surface, an
  env-guarded `hyprctl` call, stays inside `song` as its own function so a
  future `management` crate can lift just that half out later.
- **`evals`** — an eval harness: golden snapshots (existing), agent-behavior
  evals for the steward, door-contract evals, ricing/design evals.
  **Status: specified, not built, downstream of `steward`.** Today's only
  "eval" is the golden command-path snapshot test in `cli`/`lyra`'s
  `registry.rs`; door-contract evals already exist correctly, in-crate, in
  `protocol`/`server`/`client` tests; ricing/design evals are undesigned —
  design quality is a human vision-check today.

## Open questions

- **`storage` backend.** Stage/state JSON files (zero-dep, matches today's
  discipline) versus a real embedded store — deferred until the steward's
  memory needs query/search.
- **`audit` home.** Lives inside `protocol` as a shared spine; could earn its
  own crate if it grows a sink/exporter surface.
- **Flake packaging.** Whether the crate workspace ships as its own
  standalone flake (this repo's NixOS modules consuming it as an input) or
  the repo flake exposes core as a portable `packages.default` +
  `apps.default` alongside the NixOS module.
- **`management` host backend.** The NixOS backend (nixos-rebuild/modules)
  exists conceptually; the portable-Nix backend (`nix profile`/
  home-manager-style, for rebuild-switch-rice on a non-NixOS host) is new
  work, blocked on `management` itself landing.

## Related

- [[Full-Architecture]]
- [[Codebase]]
- [[Agent-Interface]]
- [[A2A-Door]]
- [[Session-Graph]]
- [[Conductor-Channel]]
- [[Self-Ricing]]
- [[Song-Vocabulary]]
- [[Governance]]
- [[Rebuild-Gate]]
- [[Secrets-Broker]]
- [[Screen-Control]]
- [[aoide-cli]]
- [[aoided]]
- [[shellbridge]]
- [[Mneme]]
- [[Melete]]
