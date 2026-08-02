# aoide · package layout (target blueprint)

> **Status: blueprint / not yet built.** This document describes the *target*
> shape of the `pkgs/aoide` Rust package — a pi-style split into single-charter
> crates — and the phased migration to reach it. No crates below exist yet
> except as the current single-crate code they will be carved from. Nothing here
> asserts present structure; the "maps-from" column is where today's code lives
> and where it will move. Reference: [earendil-works/pi](https://github.com/earendil-works/pi).

## Why

`pkgs/aoide` is one crate today (two bins — `aoide`, `aoided` — over
`src/*.rs` + `commands/` + `conductor/` + `graph/`). It works, but every
concern shares one namespace and one dependency set. pi's monorepo is the
reference for the opposite discipline: **one package, one charter, one
dependency boundary**, wired together by a thin app package at the top.

We adopt that shape *in aoide's own terms*. The non-negotiable invariant it must
preserve is **"one schema, N doors"** (CONTRACTS.md, concepts/Agent-Interface):
CLI + MCP + A2A all derive from a single registry and execute through a single
dispatcher, and must never drift. In the split, that single source becomes its
own crate (`protocol`) that every door depends on — the boundary makes the
invariant structural rather than conventional.

## What Aoide is — a distributable flake, not a NixOS captive

> **Aoide** — *she scores her own stage, conducts every agent on it, and travels
> light.* A **self-ricing, agent-conducting** environment distributed as a single
> **flake**: fork it onto any Linux and Nix provisions its own world. **NixOS
> optional, never required** — that distro is *AoideOS*; this is the muse herself.

This crate split is the *how* for a load-bearing goal: **Aoide-the-core ships as
its own package/flake, runnable on any Linux distribution through Nix — the
package manager, not the OS.** Nix runs on Arch, Debian, Fedora, macOS; AoideOS
(this repo's NixOS distribution) is one *target*, not the substrate. The muse
provisions and version-pins her *own* world (binaries, theme assets, the agent's
tools) via Nix, so a user on any distro does `nix run`/`nix profile install` and
gets the whole self-ricing, agent-conducting environment without adopting NixOS.

**Design consequences the split must honor (front-of-mind for every crate, and
for the steward especially):**

- **No NixOS assumption below `cli`.** The workspace is a standalone flake with
  no dependency on this repo's NixOS modules (nucleus/dendrites/facets). The
  NixOS integration is a *consumer* of the crates, never the other way round.
- **`management` is host-abstracted.** The privileged hands speak to a host
  *backend*: a **NixOS backend** (nixos-rebuild / modules, as today) and a
  **portable-Nix backend** (home-manager-style profile / `nix profile` / an
  Aoide-owned generation) selected at runtime by what the host actually is.
  Rebuild/switch, service control, and sudo-tracking route through that seam.
- **`song` rices portably.** Theming can't require Stylix/NixOS-module plumbing;
  the ricing engine must also apply a song on a generic-Linux host (writing the
  base16 surfaces + reloading Quickshell) so *self-ricing* holds off-NixOS.
- **The `steward` manages packages via Nix, not the distro's.** Its tools
  install/pin/upgrade through Aoide's own Nix-managed set — the same on Fedora as
  on NixOS — so the agent's capabilities are identical everywhere it's forked.
- **`conduct`/`server`/`client`/`protocol`/`storage` are already OS-neutral**
  (std + Linux syscalls + files); the split just keeps them that way.

## The pi → aoide mapping

| pi package | aoide crate | why it differs |
|---|---|---|
| `protocol` (cbor/framing/schemas) | **`protocol`** | aoide's wire is the registry-derived schema + outcome envelope + door types, not a CBOR frame codec. Same role: the contract every surface reads. |
| `ai` (multi-provider LLM API + oauth) | *(dropped)* | agents bring their own shell + model; aoide wraps no LLM API. The only remnant — active-model *detection* — is a footnote in `conduct`. |
| `agent` (loop + harness/tools/session/memory) | **`steward`** | aoide's agent is a **system-management steward**, not a coding agent: its tools act on the host (NixOS *or* any Nix-enabled Linux, via `management`'s backend seam), its memory is *design primitives*, and it is driven by the conductor. Named for its charter, not the generic `agent`. |
| `client` (connection/transport/unix/session) | **`client`** | outbound: the A2A client + the melete adapter + transports that drive external agents and talk to `aoided`. |
| `server` (listener/sessions/snapshots) | **`server`** | `aoided`: the door serve-loops (A2A/MCP), listeners, sessions, snapshots, audit sink. |
| `storage/sqlite-node` (session store + migrations + search) | **`storage`** | durable session data + memory persistence. Today's `graph/session_store.rs` + stage/state files are the seed; a real store is the target. |
| `evals` (vitest-eval harness) | **`evals`** | golden snapshots today → agent-behavior, door-contract, and ricing/design evals. |
| `tui` (terminal UI library) | **`conductor`** | aoide already has its TUI: `conductor/` (app/ui/graphview/theme). It doubles as the control surface for the steward. |
| `coding-agent` (the concrete app) | **`cli`** | the `aoide`/`aoided` bins + `commands/` that wire every crate into the two shipped binaries. |
| *(none)* | **`conduct`** | aoide-unique: the PTY multiplexer + session DAG + hook plumbing. The core that makes every terminal a tracked, conductable session. |
| *(none)* | **`song`** | aoide-unique: the ricing / design engine (notes → drachma, songs, stage, the Pantheon design language). |
| *(none)* | **`management`** | aoide-unique: the privileged **hands** — rebuild/switch, `rice mint`, hypr control, service/daemon ops, sudo-tracking. |

**Two additions beyond a literal port, both driven by this request:**
- **`agent/canon`** — the steward's memory of *design-element primitives* (how
  the user likes things built) is split out as a named concern, not buried in a
  generic memory-store. It is what the self-audit checks against.
- **`audit`** as a first-class spine (lives in `protocol`, consumed everywhere) —
  aoide's three-door security model earns an observability home, pi's
  `agent/docs/observability` elevated to a boundary.

## Target tree

```
pkgs/aoide/
  Cargo.toml                    # [workspace] — members = crates/*
  crates/
    protocol/                   # one schema, N doors — THE contract
      src/ registry · schema · envelope(Outcome) · canonical_state
           · door(Cli|Mcp|A2a) · audit · wire/{a2a-jsonrpc, mcp}
      CONTRACTS.md              # (moves here — protocol's spec)
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

| crate | charter | maps-from (today) | status |
|---|---|---|---|
| **protocol** | The single contract: registry, schema doc, `Outcome` envelope + exit codes, `canonical_state`, `Door`, audit event classes, and the A2A-JSON-RPC / MCP wire types. Every door depends on it; it depends on nothing aoide-specific. | `registry.rs`, `output.rs`, `daemon.rs`(Door/audit), a2a/mcp wire types, `CONTRACTS.md`, `commands/meta.rs`(schema) | carve-out |
| **conduct** | The session core: PTY-backed `conduct` wrap, the session DAG, hook ingestion, liveness reaping. Makes every terminal a tracked, conductable session. | `graph/` (conduct·send·model·doc·window·verbs·common), `shellbridge.rs`, `reap.rs` | carve-out |
| **server** | `aoided` and the door serve-loops: MCP-over-stdio, A2A JSON-RPC/HTTP/SSE, listeners, sessions, snapshots, the audit sink. Untrusted input stops here. | `daemon.rs`, `mcp.rs`, `a2a.rs`(serve half), `commands/infra.rs` | carve-out |
| **client** | Outbound: the A2A client registry + send, the melete adapter (neutral-event consumer), transports. Drives external agents and speaks to `aoided`. | `a2a.rs`(client half), `adapter.rs`, `commands/a2a.rs` | carve-out |
| **storage** | Durable session data + memory persistence: session store, transcript index, stage/state files, migrations, search. The steward's `canon` and session memory persist through here. | `graph/session_store.rs`, `song/stage/*`, `state/*` | seed → build |
| **steward** ★ | A system-management agent driven by the conductor. Runs a harness loop; its tools act on the host via `management`/`conduct`/`song`; it remembers design primitives in `canon`; it self-audits against canon + contracts. | *(new)* | skeleton |
| **song** | The ricing / design engine: locate `drachma`, apply songs, mint palettes, write the stage, the Pantheon design language. **Rices portably** — applies a song on generic Linux too, not only via Stylix/NixOS modules. | `notes.rs`, `commands/rice.rs`, `commands/cover.rs` | carve-out |
| **management** | The privileged hands: rebuild/switch, `rice mint`, hypr control, service/daemon ops, sudo-tracking. **Host-abstracted** — a NixOS backend (nixos-rebuild/modules) and a portable-Nix backend (`nix profile`/home-manager-style) behind one seam, chosen by what the host is. The capabilities the steward's tools invoke. | `hypr.rs`, `commands/infra.rs`(host ops), sudo-track (44c6ec9) | carve-out |
| **evals** | Eval harness: golden snapshots (existing), agent-behavior evals for the steward, door-contract evals, ricing/design evals. | golden-snapshot tests | elevate |
| **conductor** | The CLI/TUI surface: session DAG view, roster, and the steward's control panel. | `conductor/` (app·ui·graphview·theme) | carve-out |
| **cli** | The app that wires everything into `aoide` + `aoided`: arg parse, the single dispatcher, `commands/`, guide/onboarding. | `src/bin/*`, `cli.rs`, `dispatch.rs`, `commands/*`, `guide.rs`, `lib.rs` | carve-out |

## The steward (`steward`) — detail

The headline new package. What it is and is not:

- **System management, not coding.** Its job is to keep the running Aoide
  coherent — rebuild/switch, run design/ricing passes, wire gadgets, tend songs,
  watch the session DAG. Its tools are `management` verbs, not file edits in a repo.
- **Distro-agnostic by construction.** The steward manages packages, themes, and
  services through Aoide's *own* Nix-managed world via `management`'s backend
  seam — so its capabilities are identical whether it's forked onto NixOS or a
  generic Nix-on-Arch/Debian/Fedora host. It never assumes NixOS; "self-ricing,
  agent-conducting, anywhere Nix runs" is the steward's operating envelope.
- **Conductor-driven.** It is invoked and steered through the conductor CLI/TUI
  (`conductor` crate) — a first-class conductable session like any other, not a
  hidden background daemon. The human stays in the loop; the steward proposes and
  acts under the same audit + gate every door obeys.
- **`canon` — memory of design primitives.** The user has a design language
  (Pantheon stele grammar, shade-glyph meters, box-drawing frames, palette-driven
  signatures, `gadgetW = 360`, light-only vision-check, Greek-key meander …). The
  canon persists these as structured primitives so the steward *re-applies* them
  instead of re-deriving them each pass. It is the durable form of the design
  notes in `~/.claude` memory + the wiki design-language pages, owned by the
  steward and consulted before any design work.
- **`audit` — self-auditing.** After acting, the steward checks its own output
  against the canon (did this honor the primitives?), against CONTRACTS (did it
  keep the invariants?), and against the door-security model, emitting an audit
  trail through `protocol`'s audit spine. This is the machine analog of the
  Fable review step in the orchestration pipeline.
- **Session data → `storage`.** Turn history, canon records, and audit trails
  persist through the `storage` crate, not ad-hoc files.

## Phased migration (build stays green each phase)

Each phase is one reviewable unit under the standard pipeline (Sonnet execute →
Fable/Opus review → land), with `cargo build` + targeted tests + `nix build`
green before the next begins. The two shipped binaries never change behavior;
this is structure, not features.

- **Phase 0 — blueprint.** *(this document + the wiki mirror.)* No code.
- **Phase 1 — workspace scaffold.** Turn `pkgs/aoide` into a `[workspace]` with
  the existing crate as the sole member. Nix build green. Purely mechanical.
- **Phase 2 — extract `protocol`.** Most depended-on, least behavioral risk:
  registry, `Outcome`, `Door`, `canonical_state`, audit classes, wire types.
  Every other module now imports it.
- **Phase 3 — extract `conduct` + seed `storage`.** `graph/` + `shellbridge` +
  `reap` → `conduct`; `graph/session_store` → `storage`.
- **Phase 4 — extract `server` + `client`.** Split `a2a.rs` at the serve/client
  seam; `daemon`/`mcp` → `server`; `adapter` → `client`.
- **Phase 5 — extract `song` + `management`.** `notes`/`rice`/`cover` → `song`;
  `hypr`/host-ops/sudo-track → `management`.
- **Phase 6 — extract `conductor` + `cli`.** `conductor/` → its crate; bins +
  `commands/` + `cli`/`dispatch` → the top `cli` app crate.
- **Phase 7 — build `steward` (the agent).** New crate against
  protocol + conduct + storage + management: harness → tools → canon → audit →
  skills. The first genuinely new capability, not a carve-out.
- **Phase 8 — elevate `evals`.** Move golden snapshots in; add steward-behavior,
  door-contract, and ricing/design eval suites.

## Open questions (each tagged with when it must be settled)

- **Naming — DECIDED (hybrid).** Plain names where the concept is universal
  (`protocol`, `server`, `client`, `storage`, `evals`, `cli`); the already-coined
  aoide vocabulary kept (`conduct`, `song`, `conductor`); and voice only where the
  crate is aoide's own — the agent is **`steward`** (with `canon` + `audit`
  inside). Rejected: fully-literal (`agent`), the stagecraft scheme
  (`podium`/`cue`/…), and the deep-pantheon scheme (`nomos`/`tekton`/…) — the
  latter two taxed grep-ability on universal crates for no semantic gain, and
  `mneme`/`melete` collide with live MCP servers.
- **`storage` backend.** stage/state JSON files (zero-dep, matches today's
  discipline) vs. a real embedded store. pi uses sqlite; aoide's zero-dep rule
  argues for staying file-first until the steward's memory needs query/search.
  *(Deferred to Phase 3 by decision — Phases 1–2 don't touch storage internals.)*
- **`audit` home.** Kept inside `protocol` as a shared spine here; could be its
  own crate if it grows a sink/exporter surface.
- **Flake packaging (portability).** The workspace ships as its **own standalone
  flake** — `nix run`/`nix profile install` on any Linux yields the whole
  environment — with this repo's NixOS modules a *consumer* of it. Decide the
  boundary: does the crate workspace get its own `flake.nix` (repo consumes it as
  an input), or does the repo flake expose Aoide-core as a portable
  `packages.default` + `apps.default` alongside the NixOS module? Author Phase 1's
  workspace so this stays open (no NixOS-module dependency reaches into the crates).
- **`management` host backend.** The NixOS backend (nixos-rebuild/modules) exists
  today; the **portable-Nix backend** (home-manager-style profile / `nix profile`
  / an Aoide-owned generation for rebuild-switch-rice on a non-NixOS host) is new
  work — scope it when `management` is carved out (Phase 5), plus how the backend
  is detected at runtime (are we on NixOS, or generic-Nix?).
