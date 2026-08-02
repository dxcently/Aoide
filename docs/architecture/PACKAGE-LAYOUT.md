# aoide · package layout (target blueprint)

> **Status: the crate restructure is complete.** Phases 1 through 6b are
> landed: `pkgs/aoide` is now a Cargo workspace with seven crates carved out —
> `protocol`, `storage`, `conduct`, `client`, `server`, `song`, `conductor` —
> plus the root `aoide`/`aoided` binaries. `management` was paired with `song`
> in the original Phase 5 plan but was deferred, not dropped (see the Phase 5
> status note below). Phase 6 planning found `cli` isn't a separate crate to
> build — the root `aoide` package already is the DAG sink (depends on
> everything, nothing depends on it), so only `conductor` extracted; landed as
> two sub-phases (6a: a dependency-injection seam so the TUI never needs the
> trunk's assembled command registry; 6b: the mechanical move). Phase 7
> (`steward`) and Phase 8 (`evals`) were both planned and the verdict on each
> is DEFER (see their status notes below) — nothing else is queued to extract.
> Reference: [earendil-works/pi](https://github.com/earendil-works/pi).

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
| **protocol** | The single contract: registry, schema doc, `Outcome` envelope + exit codes, `canonical_state`, `Door`, audit event classes, and the A2A-JSON-RPC / MCP wire types. Every door depends on it; it depends on nothing aoide-specific. | `registry.rs`, `output.rs`, `daemon.rs`(Door/audit), a2a/mcp wire types, `CONTRACTS.md`, `commands/meta.rs`(schema) | landed (Phase 2) |
| **conduct** | The session core: PTY-backed `conduct` wrap, the session DAG, hook ingestion, liveness reaping. Makes every terminal a tracked, conductable session. | `graph/` (conduct·send·model·doc·window·verbs·common), `shellbridge.rs`, `reap.rs` | landed (Phase 3b) |
| **server** | `aoided` and the door serve-loops: MCP-over-stdio, A2A JSON-RPC/HTTP/SSE, listeners, sessions, snapshots, the audit sink. Untrusted input stops here. | `daemon.rs`, `mcp.rs`, `a2a.rs`(serve half), `commands/infra.rs` | landed (Phase 4c) |
| **client** | Outbound: the A2A client registry + send, the melete adapter (neutral-event consumer), transports. Drives external agents and speaks to `aoided`. | `a2a.rs`(client half), `adapter.rs`, `commands/a2a.rs` | landed (Phase 4b) |
| **storage** | Durable session data + memory persistence: session store, transcript index, stage/state files, migrations, search. The steward's `canon` and session memory persist through here. | `graph/session_store.rs`, `song/stage/*`, `state/*` | landed (Phase 3a); backend is still file-first (seed → build) |
| **steward** ★ | A system-management agent driven by the conductor. Runs a harness loop; its tools act on the host via `management`/`conduct`/`song`; it remembers design primitives in `canon`; it self-audits against canon + contracts. | *(new)* | skeleton — DEFER (Phase 7) |
| **song** | The ricing / design engine: locate `drachma`, apply songs, mint palettes, write the stage, the Pantheon design language. **Rices portably** — applies a song on generic Linux too, not only via Stylix/NixOS modules. | `notes.rs`, `commands/rice.rs`, `commands/cover.rs` | landed (Phase 5a+5b) |
| **management** | The privileged hands: rebuild/switch, `rice mint`, hypr control, service/daemon ops, sudo-tracking. **Host-abstracted** — a NixOS backend (nixos-rebuild/modules) and a portable-Nix backend (`nix profile`/home-manager-style) behind one seam, chosen by what the host is. The capabilities the steward's tools invoke. | `hypr.rs`, `commands/infra.rs`(host ops), sudo-track (44c6ec9) | carve-out — DEFERRED out of Phase 5 (no ETA) |
| **evals** | Eval harness: golden snapshots (existing), agent-behavior evals for the steward, door-contract evals, ricing/design evals. | golden-snapshot tests | elevate — DEFER (Phase 8) |
| **conductor** | The CLI/TUI surface: session DAG view, roster, and the steward's control panel. | `crates/conductor/` (app·ui·graphview·theme) | landed (Phase 6a+6b) |
| **cli** | The app that wires everything into `aoide` + `aoided`: arg parse, the single dispatcher, `commands/`, guide/onboarding. **Not a separate crate** — this is the root `aoide` package itself (the DAG sink: depends on everything, nothing depends on it; a `crates/cli` layer would enforce no invariant, see the Phase 6 status note). | `src/bin/*`, `cli.rs`, `dispatch.rs`, `commands/*`, `guide.rs`, `lib.rs` | **is** the root package (Phase 6 finding) |

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
- **Phase 1 — workspace scaffold. LANDED (e6809f6).** Turn `pkgs/aoide` into a
  `[workspace]` with the existing crate as the sole member. Nix build green.
  Purely mechanical.
- **Phase 2 — extract `protocol`. LANDED (fd9bc1e).** Most depended-on, least
  behavioral risk: registry, `Outcome`, `Door`, `canonical_state`, audit
  classes, wire types. Every other module now imports it.
- **Phase 3a — extract `storage`. LANDED (5691985).** `graph/session_store` +
  stage/state files → `storage`.
- **Phase 3b — extract `conduct`. LANDED (9c41f2a).** `graph/` +
  `shellbridge` + `reap` → `conduct`. (The original blueprint listed this as
  one Phase 3 — "extract `conduct` + seed `storage`" — landed as two units,
  `storage` first.)
- **Phase 4a — typed wire structs. LANDED (38f9c62).** A2A/MCP payloads gain
  typed structs in `protocol`, ahead of the client/server split.
- **Phase 4b — extract `client`. LANDED (0508e76).** `adapter` + the A2A
  client half → `client`.
- **Phase 4c — extract `server`. LANDED (5ae9f6d).** `daemon`/`mcp` + the A2A
  serve half → `server`. (The original blueprint listed this as one Phase 4 —
  landed as three units.)
- **Phase 5a — birth `song`. LANDED (de63880).** `notes.rs` → `song`, as a
  leaf crate with `notes` + `live` modules (no `storage` dependency yet).
- **Phase 5b — `song` gains `mint` + `cover`. LANDED (c56faa7).**
  `commands/rice.rs` + `commands/cover.rs` fold in as `song::mint` /
  `song::cover`, adding a `storage` dependency.
- **`management` — DEFERRED out of Phase 5, indefinitely.** The blueprint
  originally paired `management` with `song` as one phase ("extract `song` +
  `management`"). Planning research for Phase 5 found `management`'s charter
  is largely *aspirational*, not just unextracted: there is no
  `nixos-rebuild`/`switch`/`systemctl` code anywhere in the Rust binary today
  — by deliberate house policy (rebuild is user-gated, no self-updaters; see
  `AGENTS.md`). The only real host-effect anywhere in the ricing surface is
  one env-guarded `hyprctl` call, which stayed inside `song::live` —
  specifically kept as a separate function from
  `song::live::geometry_keywords` so a future `management` crate could lift
  just the host-effect half out later without touching the pure computation.
  `management` is deferred indefinitely until real host-ops verbs
  (rebuild/switch/service control) actually get built — there's no ETA, it's
  not "next," it's "whenever that work exists to extract."
- **Phase 6 — extract `conductor`; `cli` stays the root package. LANDED
  (6a: d6b771d, 6b: 1f5ae31).** Planning corrected the original scope: `cli`
  was never a separate crate to build — the root `aoide` package already is
  the trunk (depends on everything, nothing depends on it), so a
  `crates/cli` layer would be ceremony, not architecture. `conductor`
  extracted cleanly over `{protocol, conduct, storage}` — deliberately not
  `server` — via a dependency-injection seam (6a): the TUI's `App` takes a
  `DispatchFn` fn-pointer parameter instead of calling the trunk's assembled
  `dispatch::dispatch` directly, so it never needs the registry singleton
  that `server` also couldn't depend on directly back in Phase 4c. Not
  parallelizable (one entangled module, not independent files) — landed as
  two sequential sub-phases by a single executor each time, not multiple
  agents at once. **This is the last phase of the crate restructure** —
  `pkgs/aoide` is one crate → eight (protocol/storage/conduct/client/server/
  song/conductor + the root trunk); nothing else is queued to extract.
- **Phase 7 — build `steward` (the agent). DEFER.** New crate against
  protocol + conduct + storage + management: harness → tools → canon → audit →
  skills. The first genuinely new capability, not a carve-out. Verdict: DEFER.
  Two of its five pillars are blocked: `harness` (a native LLM turn-loop)
  would require aoide to adopt a model client it has explicitly refused to
  own — "any agent with a shell is fully capable" is the core thesis, and
  aoide wraps no LLM API; `tools`/`skills` need `management`'s host-ops verbs,
  which don't exist (see above). The one real raw material — a rich
  design-decision corpus already on disk (`song/songbook/*/design/*.md`, the
  wiki design-language pages) that `canon` would formalize — isn't worth a
  crate yet, because the docs are prose, not structured/checkable records; a
  `canon show` verb today would just be `cat` with a registry entry wrapped
  around it. Unblock conditions, in priority order: (1) a concrete decision
  from khoa on whether `steward` is LLM-driven itself or a verb-surface an
  external agent drives via `conduct`; (2) `management` actually built;
  (3) a decision to formalize canon into typed primitives.
- **Phase 8 — elevate `evals`. DEFER, strictly downstream of Phase 6 and
  Phase 7.** Move golden snapshots in; add steward-behavior, door-contract,
  and ricing/design eval suites. The blueprint's "golden snapshots
  (existing)" turned out to be exactly one 50-line test
  (`command_paths_match_the_golden_snapshot` in `pkgs/aoide/src/registry.rs`)
  — no `evals/` crate, no fixture files, no harness of any kind exists
  anywhere; every other "eval" string in the codebase means Nix eval,
  unrelated. Of the four planned suites: golden snapshots (trivial, and its
  future home crate `cli` doesn't exist until Phase 6 lands); agent-behavior
  evals (blocked — no `steward` exists); door-contract evals (already exist
  correctly, in-crate, in `protocol`/`server`/`client` tests — nothing to
  extract); ricing/design evals (undesigned research — no scoring exists or
  is designed, design quality is a human vision-check today). The
  blueprint's own steward section assigns "formalize the review-pipeline
  self-check" to `steward/audit`, not `evals` — so this is explicitly a
  different concept from either.

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
  work. **`management` itself was deferred out of Phase 5**, indefinitely (see
  the Phase 5 status note above) — this question stays open until real
  host-ops verbs actually get built and `management` is carved out, plus how
  the backend is detected at runtime (are we on NixOS, or generic-Nix?).
