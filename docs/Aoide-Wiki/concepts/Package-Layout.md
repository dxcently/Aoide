---
type: concept
created: 2026-08-01
updated: 2026-08-01
tags: [aoide, architecture, rust, cli, crate, blueprint]
---

# Package Layout — the pi-Style Crate Split (Target Blueprint)

**Status: specified; not built.** `pkgs/aoide` is **one crate today** — two
bins (`aoide`, `aoided`) over `src/*.rs` + `commands/` + `conductor/` +
`graph/`. This page documents the *target* shape of that package: a
[pi](https://github.com/earendil-works/pi)-style split into single-charter
crates under a `[workspace]`, plus the phased migration to reach it. Nothing
below exists as a crate yet; the "maps-from" and "status" columns name where
today's code lives and what carving it out still requires. Grounded in the
repo's `docs/architecture/PACKAGE-LAYOUT.md`, which this page mirrors in the
wiki's own voice — read it for the full text, including the open questions
this page only summarizes.

## Why split at all

pi's monorepo is the reference for a discipline aoide's single crate does not
yet have: **one package, one charter, one dependency boundary**, wired
together by a thin app package at the top. Aoide adopts that shape in its own
terms, and the shape is not incidental — it makes the CONTRACTS.md invariant
**"one schema, N doors"** ([[Agent-Interface]]: CLI trunk + MCP façade +
[[A2A-Door]] all deriving from one registry, never drifting) *structural*
rather than conventional. In the split, that single source becomes its own
crate (`protocol`) that every door depends on.

Two things go beyond a literal pi port, both driven by aoide's own shape:

- **`steward/canon`** splits out as a named concern — the steward's memory of
  *design-element primitives* is not buried in a generic memory store.
- **`audit`** is elevated to a first-class spine (living inside `protocol`,
  consumed everywhere) — pi's `agent/docs/observability` becomes a boundary,
  matching aoide's three-door security model.

## The pi → aoide mapping

| pi package | aoide crate | why it differs |
|---|---|---|
| `protocol` (cbor/framing/schemas) | **`protocol`** | aoide's wire is the registry-derived schema + outcome envelope + door types, not a CBOR frame codec — same role: the contract every surface reads |
| `ai` (multi-provider LLM API + oauth) | *(dropped)* | agents bring their own shell + model; aoide wraps no LLM API — active-model detection is a footnote in `conduct` |
| `agent` (loop + harness/tools/session/memory) | **`steward`** | aoide's agent is a **system-management steward**, not a coding agent — its tools act on the NixOS host, its memory is *design primitives*, and it is conductor-driven; named for its charter, not the generic `agent` |
| `client` (connection/transport/unix/session) | **`client`** | outbound: the A2A client + the melete adapter + transports that drive external agents and talk to `aoided` |
| `server` (listener/sessions/snapshots) | **`server`** | `aoided`: the door serve-loops (A2A/MCP), listeners, sessions, snapshots, audit sink |
| `storage`/`sqlite-node` (session store + migrations + search) | **`storage`** | durable session data + memory persistence; today's `graph/session_store.rs` + stage/state files are the seed |
| `evals` (vitest-eval harness) | **`evals`** | golden snapshots today → agent-behavior, door-contract, and ricing/design evals |
| `tui` (terminal UI library) | **`conductor`** | aoide already has its TUI: `conductor/` (app/ui/graphview/theme), doubling as the steward's control surface |
| `coding-agent` (the concrete app) | **`cli`** | the `aoide`/`aoided` bins + `commands/` wiring every crate into the two shipped binaries |
| *(none)* | **`conduct`** | aoide-unique: the PTY multiplexer + session DAG + hook plumbing — the core that makes every terminal a tracked, [[Conductor-Channel|conductable]] session |
| *(none)* | **`song`** | aoide-unique: the ricing/design engine — notes → drachma, songs, stage, the Pantheon design language ([[Self-Ricing]], [[Song-Vocabulary]]) |
| *(none)* | **`management`** | aoide-unique: the privileged **hands** — rebuild/switch, `rice mint`, hypr control, service/daemon ops, sudo-tracking ([[Rebuild-Gate]]) |

## Target crate tree

```
pkgs/aoide/
  Cargo.toml                    # [workspace] — members = crates/*
  crates/
    protocol/                   # one schema, N doors — THE contract
      src/ registry · schema · envelope(Outcome) · canonical_state
           · door(Cli|Mcp|A2a) · audit · wire/{a2a-jsonrpc, mcp}
      CONTRACTS.md               # moves here — protocol's spec
    conduct/                    # PTY multiplexer + session DAG + hooks
      src/ conduct · send · model · doc · window · verbs · reap · shellbridge
    server/                     # aoided — the daemon + door serve-loops
      src/ daemon · listener · mcp-serve · a2a-serve · sessions · snapshots
    client/                     # outbound: drive external agents / talk to aoided
      src/ a2a-client · adapter(melete) · transport · session-handle
    storage/                    # durable session data + memory persistence
      src/ session-store · transcript-index · stage-state · migrations · search
    steward/             ★NEW   # the STEWARD — system-management agent
      src/ harness/      —  the run loop (turn loop, tool dispatch, compaction)
           tools/        —  system-management verbs (wraps management/conduct/song)
           canon/        —  memory of DESIGN PRIMITIVES + how-the-user-likes-things
           audit/        —  self-auditing (checks its work vs canon + contracts)
           skills/       —  packaged procedures (rebuild · mint a song · wire a gadget)
    song/                       # ricing / design engine
      src/ notes(drachma-locate) · rice · cover · stage · palette
    management/                 # privileged host-ops — the hands
      src/ hypr · infra · rebuild · service · sudo-track
    evals/               ★NEW   # eval harness
      src/ golden-snapshot · agent-behavior · door-contract · ricing-design
    conductor/                  # the CLI/TUI surface (drives sessions + steward)
      src/ app · ui · graphview · theme
    cli/                        # the app: aoide/aoided bins + commands
      src/ bin/{aoide,aoided} · cli(parse) · dispatch · commands/* · guide
```

## Per-crate charter

Each row's **status** names where the split stands: `carve-out` means the
code exists today and only needs to move; `skeleton` means the crate is a new
build (no existing code to carve); `seed → build` means today's code is a
partial stand-in for the real thing; `elevate` means promoting an existing
mechanism to crate status.

| crate | charter | maps-from (today) | status |
|---|---|---|---|
| **protocol** | The single contract: registry, schema doc, `Outcome` envelope + exit codes, `canonical_state`, `Door`, audit event classes, and the A2A-JSON-RPC/MCP wire types. Every door depends on it; it depends on nothing aoide-specific. | `registry.rs`, `output.rs`, `daemon.rs`(Door/audit), a2a/mcp wire types, `CONTRACTS.md`, `commands/meta.rs`(schema) | carve-out |
| **conduct** | The session core: PTY-backed `conduct` wrap, the session DAG, hook ingestion, liveness reaping — makes every terminal a tracked, conductable session. | `graph/` (conduct·send·model·doc·window·verbs·common), `shellbridge.rs`, `reap.rs` | carve-out |
| **server** | `aoided` and the door serve-loops: MCP-over-stdio, A2A JSON-RPC/HTTP/SSE, listeners, sessions, snapshots, the audit sink. Untrusted input stops here. | `daemon.rs`, `mcp.rs`, `a2a.rs`(serve half), `commands/infra.rs` | carve-out |
| **client** | Outbound: the A2A client registry + send, the melete adapter (neutral-event consumer), transports — drives external agents and speaks to `aoided`. | `a2a.rs`(client half), `adapter.rs`, `commands/a2a.rs` | carve-out |
| **storage** | Durable session data + memory persistence: session store, transcript index, stage/state files, migrations, search. The steward's `canon` and session memory persist through here. | `graph/session_store.rs`, `song/stage/*`, `state/*` | seed → build |
| **steward** ★ | A system-management agent driven by the conductor. Runs a harness loop; its tools act on the host via `management`/`conduct`/`song`; it remembers design primitives in `canon`; it self-audits against canon + contracts. | *(new)* | skeleton |
| **song** | The ricing/design engine: locate `drachma`, apply songs, mint palettes, write the stage, the Pantheon design language. | `notes.rs`, `commands/rice.rs`, `commands/cover.rs` | carve-out |
| **management** | The privileged hands: rebuild/switch, `rice mint`, hypr control, service/daemon ops, sudo-tracking — the capabilities the steward's tools invoke. | `hypr.rs`, `commands/infra.rs`(host ops), sudo-track | carve-out |
| **evals** | Eval harness: golden snapshots (existing), agent-behavior evals for the steward, door-contract evals, ricing/design evals. | golden-snapshot tests | elevate |
| **conductor** | The CLI/TUI surface: session DAG view, roster, and the steward's control panel. | `conductor/` (app·ui·graphview·theme) | carve-out |
| **cli** | The app that wires everything into `aoide` + `aoided`: arg parse, the single dispatcher, `commands/`, guide/onboarding. | `src/bin/*`, `cli.rs`, `dispatch.rs`, `commands/*`, `guide.rs`, `lib.rs` | carve-out |

## The steward (`steward` crate) — detail

The blueprint's headline new package, ★NEW / skeleton status. What the design
specifies:

- **System management, not coding.** Its job is keeping AoideOS coherent —
  rebuild/switch, run design/ricing passes, wire gadgets, tend songs, watch
  the session DAG. Its tools are `management` verbs, not file edits in a repo.
- **Conductor-driven.** It is invoked and steered through the `conductor`
  crate's CLI/TUI — a first-class [[Conductor-Channel|conductable session]]
  like any other, not a hidden background daemon. The human stays in the
  loop; the steward proposes and acts under the same audit + gate every door
  obeys ([[Governance]]).
- **`canon` — memory of design primitives.** The user has a design language
  (the Pantheon stele grammar, shade-glyph meters, box-drawing frames,
  palette-driven signatures, `gadgetW = 360`, light-only vision-check, the
  Greek-key meander, …). `canon` persists these as structured primitives so
  the steward *re-applies* them instead of re-deriving them each pass. It is
  the durable form of the design notes that live in `~/.claude` memory and
  this wiki's design-language pages today, owned by the steward and consulted
  before any design work.
- **`audit` — self-auditing.** After acting, the steward checks its own
  output against `canon` (did this honor the primitives?), against
  CONTRACTS.md (did it keep the invariants?), and against the door-security
  model, emitting an audit trail through `protocol`'s audit spine. This is
  the machine analog of the review step in the human orchestration pipeline.
- **Session data → `storage`.** Turn history, canon records, and audit trails
  persist through the `storage` crate, not ad-hoc files.

## Phased migration

The design specifies nine phases, build green throughout: each is one
reviewable unit (execute → review → land) with `cargo build` + targeted tests
+ `nix build` green before the next begins. The two shipped binaries (`aoide`,
`aoided`) never change behavior across any phase — this is structure, not
features.

| phase | scope |
|---|---|
| **0 — blueprint** | `docs/architecture/PACKAGE-LAYOUT.md` + this wiki mirror. No code. |
| **1 — workspace scaffold** | Turn `pkgs/aoide` into a `[workspace]` with the existing crate as the sole member. Nix build green. Purely mechanical. |
| **2 — extract `protocol`** | Most depended-on, least behavioral risk: registry, `Outcome`, `Door`, `canonical_state`, audit classes, wire types. Every other module imports it. |
| **3 — extract `conduct` + seed `storage`** | `graph/` + `shellbridge` + `reap` → `conduct`; `graph/session_store` → `storage`. |
| **4 — extract `server` + `client`** | Split `a2a.rs` at the serve/client seam; `daemon`/`mcp` → `server`; `adapter` → `client`. |
| **5 — extract `song` + `management`** | `notes`/`rice`/`cover` → `song`; `hypr`/host-ops/sudo-track → `management`. |
| **6 — extract `conductor` + `cli`** | `conductor/` → its own crate; bins + `commands/` + `cli`/`dispatch` → the top `cli` app crate. |
| **7 — build `steward`** | New crate against `protocol` + `conduct` + `storage` + `management`: harness → tools → canon → audit → skills. The first genuinely new capability, not a carve-out. |
| **8 — elevate `evals`** | Move golden snapshots in; add steward-behavior, door-contract, and ricing/design eval suites. |

## Open questions

**Naming — DECIDED (hybrid).** Plain names where the concept is universal
(`protocol`, `server`, `client`, `storage`, `evals`, `cli`); the already-coined
aoide vocabulary kept (`conduct`, `song`, `conductor`); voice used only where
the crate is aoide's own — the agent is **`steward`** (`canon` + `audit`
inside). Rejected: fully-literal (`agent`), the stagecraft scheme
(`podium`/`cue`/…), and the deep-pantheon scheme (`nomos`/`tekton`/…) — the
latter two taxed grep-ability on universal crates for no semantic gain, and
`mneme`/`melete` collide with aoide's own live MCP servers ([[Mneme]],
[[Melete]]).

Two questions stay open before Phase 1 begins (full detail in the source
doc): whether `storage` stays file-first (matching today's zero-dep
discipline) or moves to a real embedded store; and whether `audit` stays
inside `protocol` or earns its own crate.

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
- [[aoide-cli]]
- [[aoided]]
- [[shellbridge]]
- [[Mneme]]
- [[Melete]]
