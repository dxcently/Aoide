# aoide · package layout (target blueprint)

> **Status: the crate restructure is complete.** Phases 1 through 6b are
> landed, and Phase 9 finished the job: every domain crate now owns its CLI
> commands (`commands` modules), the app shell is `crates/cli` (package
> `aoide-cli`, lib `aoide`), and the root `Cargo.toml` is a VIRTUAL workspace
> with one `[workspace.dependencies]` version source — `src/` no longer
> exists. `management` was paired with `song`
> in the original Phase 5 plan but was deferred, not dropped (see the Phase 5
> status note below). Phase 6 planning found `cli` isn't a separate crate to
> build — the root `aoide` package already is the DAG sink (depends on
> everything, nothing depends on it), so only `conductor` extracted; landed as
> two sub-phases (6a: a dependency-injection seam so the TUI never needs the
> trunk's assembled command registry; 6b: the mechanical move). **Phase 9
> deliberately superseded that finding** once the handlers left the root —
> see its status note. Phase 7
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
> **flake**: clone it onto any Linux and Nix provisions its own world. **NixOS
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
  no dependency on this repo's NixOS modules (nucleus/dendrites/aggregations).
  The
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
  on NixOS — so the agent's capabilities are identical everywhere it's cloned.
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
| *(none)* | **`song`** | aoide-unique: the ricing / design engine (notes → livery, songs, stage, the Pantheon design language). |
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
  Cargo.toml                    # VIRTUAL [workspace] — members = crates/*,
                                # one [workspace.dependencies] version source (Phase 9)
  crates/
    protocol/                   # one schema, N doors — THE contract
      src/ registry · schema · envelope(Outcome) · canonical_state
           · door(Cli|Mcp|A2a) · audit · feed · wire/{a2a-jsonrpc, mcp}
           · cmd!/arg!/flag! registration macros (Phase 9)
      CONTRACTS.md              # (moves here — protocol's spec)
    conduct/                    # PTY multiplexer + session DAG + hooks
      src/ conduct · send · model · doc · window · manage · reap · shellbridge
           · commands/ (graph*, conduct, hooks install — Phase 9)
    server/                     # aoided — the daemon + door serve-loops
      src/ daemon · listener · mcp-serve · a2a-serve · sessions · snapshots
           · commands (daemon, shellbridge, a2a serve — Phase 9)
    client/                     # outbound: drive node aoides / talk to aoided
      src/ wire(A2A client half) · node · adapter(melete) · discover
           · commands (node*, adapter melete — Phase 9)
    storage/                    # durable session data + memory persistence
      src/ session-store · transcript-index · stage-state · migrations · search
           · commands (usage — Phase 9)
    secrets/                    # the secrets broker — policy-gated resolve
      src/ policy · broker · client · backend · enroll(totp) · store · socket
           · commands.rs (secrets * — broker admin + resolve commands)
    test-support/        (Phase 9)  # shared test rig — DEV-dependency only
      src/ scratch-dirs · EnvSaver · fixture payloads · env_lock
    upkeep/                     # mechanical integrity — the working-tree gap
                                # `nix flake check` structurally can't see
      src/ scan · commands (soundcheck) · checklane (session-hook wired)
    steward/             ★NEW   # the STEWARD — system-management agent
      src/ harness/      —  the run loop (turn loop, tool dispatch, compaction)
           tools/        —  system-management commands (wraps management/conduct/song)
           canon/        —  memory of DESIGN PRIMITIVES + how-the-user-likes-things
           audit/        —  self-auditing (checks its work vs canon + contracts)
           skills/       —  packaged procedures (rebuild · mint a song · wire a gadget)
    song/                       # ricing / design engine
      src/ livery · notes · rice · cover · stage · palette
           · commands/ (rice*, cover set — Phase 9)
    management/                 # privileged host-ops — the hands
      src/ hypr · infra · rebuild · service · sudo-track
    evals/               ★NEW   # eval harness
      src/ golden-snapshot · agent-behavior · door-contract · ricing-design
    conductor/                  # the CLI/TUI surface (drives sessions + steward)
      src/ app · ui · graphview · theme · commands (conductor — Phase 9)
    cli/                        # the app: aoide/aoided bins + registry assembly
      src/ bin/{aoide,aoided} · cli(parse) · dispatch · guide
           · commands/ (ONLY the root-coupled groups: meta, stubs, mcp serve)
      tests/ conductor_integration (+ fixtures)
    screen/                     # read-only desktop view + pointer synthesis — lyra-only
      src/ hypr · capture · point/synth · ocr/text · diff · commands
    lyra/                       # the paint app: lyra bin, second composition
                                # root over the rice/screen/quickshell bundle
      src/ bin/lyra · dispatch/registry(own golden) · guide
           · commands/ (song*, screen*, shellbridge/herald regs, onboard)
```

## Per-crate charter

| crate | charter | maps-from (today) | status |
|---|---|---|---|
| **protocol** | The single contract: registry, schema doc, `Outcome` envelope + exit codes, `canonical_state`, `Door`, audit event classes, the `feed` primitives (`FeedWriter`/`Follower` — the append-only JSON-lines event-feed spine, same cross-cutting shape as `audit`), the A2A-JSON-RPC / MCP wire types, and the `cmd!`/`arg!`/`flag!` registration macros. Every door depends on it; it depends on nothing aoide-specific. | `registry.rs`, `output.rs`, `daemon.rs`(Door/audit), a2a/mcp wire types, `CONTRACTS.md`, `commands/meta.rs`(schema) | landed (Phase 2); macros + `audit_log_path` folded in (Phase 9) |
| **conduct** | The session core: PTY-backed `conduct` wrap, the session DAG, hook ingestion, liveness reaping — plus its CLI commands (`graph *`, `conduct`, `hooks install`). Makes every terminal a tracked, conductable session. | `graph/` (conduct·send·model·doc·window·manage·common), `shellbridge.rs`, `reap.rs`, `commands/graph.rs`, `commands/hooks.rs` | landed (Phase 3b); commands landed (Phase 9) |
| **server** | `aoided` and the door serve-loops: MCP-over-stdio, A2A JSON-RPC/HTTP/SSE, listeners, sessions, snapshots, the audit sink — plus its CLI commands (`daemon`, `shellbridge`, `a2a serve`). Untrusted input stops here. | `daemon.rs`, `mcp.rs`, `a2a.rs`(serve half), `commands/infra.rs`(server half), `commands/a2a.rs`(serve command) | landed (Phase 4c); commands landed (Phase 9) |
| **client** | Outbound: the A2A client half (card fetch, signed `message/send` to nodes), the melete adapter (neutral-event consumer), LAN discovery — plus its CLI commands (`node *`, `adapter melete`). Drives node aoides and speaks to `aoided`. | `wire.rs`(A2A client half), `node.rs`, `adapter.rs`, `discover.rs`, `commands.rs`(node commands, adapter command) | landed (Phase 4b); commands landed (Phase 9) |
| **storage** | Durable session data + memory persistence: session store, transcript index, stage/state files, migrations, search — plus its CLI command (`usage`). The steward's `canon` and session memory persist through here. | `graph/session_store.rs`, `state/stage/*`, `state/*`, `commands/usage.rs` | landed (Phase 3a); backend is still file-first (seed → build); commands landed (Phase 9) |
| **secrets** | The secrets broker: policy-gated resolve over a unix socket, TOTP enrollment, pluggable backends (`file` built-in), admin commands — plus its CLI commands (`secrets *`). A secret's value never crosses into a `Serialize`/`Deserialize` type; audit happens broker-side only. | `crates/secrets/` (broker·client·policy·backend·enroll·store·commands) | landed (Workstream SECRETS, P-V1–P-V4f) |
| **test-support** | Shared test rig (scratch dirs, `EnvSaver`, fixture payloads, the single `env_lock`). **Dev-dependency only** — never a production edge. | `commands/mod.rs::test_support`, root `env_lock` | landed (Phase 9) |
| **upkeep** | Mechanical integrity on the working tree: the gap `nix flake check` structurally can't see (gitignored/uncommitted state, a stray file at repo root, an untracked `.nix` file). `aoide soundcheck` (human/agent-invoked report) and `checklane` (the same gap wired into the agent loop via `session hook`'s SessionStart/Stop/UserPromptSubmit events) — plus its CLI command (`soundcheck`). Report-only, forever: neither ever moves, deletes, formats, or repairs. | *(new)* | landed |
| **steward** ★ | A system-management agent driven by the conductor. Runs a harness loop; its tools act on the host via `management`/`conduct`/`song`; it remembers design primitives in `canon`; it self-audits against canon + contracts. | *(new)* | skeleton — DEFER (Phase 7) |
| **song** | The ricing / design engine: the native livery engine (`src/livery/` — schema · resolve · emit; formerly a standalone Node package), apply songs, mint palettes, write the stage, the Pantheon design language — plus its CLI commands (`rice *`, `livery *`, `cover set`). **Rices portably** — applies a song on generic Linux too, not only via Stylix/NixOS modules. | `notes.rs`, `commands/rice.rs`, `commands/cover.rs` | landed (Phase 5a+5b); commands landed (Phase 9) |
| **management** | The privileged hands: rebuild/switch, `rice mint`, hypr control, service/daemon ops, sudo-tracking. **Host-abstracted** — a NixOS backend (nixos-rebuild/modules) and a portable-Nix backend (`nix profile`/home-manager-style) behind one seam, chosen by what the host is. The capabilities the steward's tools invoke. | `hypr.rs`, `commands/infra.rs`(host ops), sudo-track (44c6ec9) | carve-out — DEFERRED out of Phase 5 (no ETA) |
| **evals** | Eval harness: golden snapshots (existing), agent-behavior evals for the steward, door-contract evals, ricing/design evals. | golden-snapshot tests | elevate — DEFER (Phase 8) |
| **conductor** | The CLI/TUI surface: session DAG view, roster, and the steward's control panel — plus its one CLI command (`conductor`). | `crates/conductor/` (app·ui·graphview·theme), `commands/infra.rs`(conductor command) | landed (Phase 6a+6b); commands landed (Phase 9) |
| **cli** | The app that wires everything into `aoide` + `aoided`: arg parse, the single dispatcher, registry assembly (`commands/mod.rs::all()`), guide/onboarding, and the three root-coupled groups (`meta`, `stubs`, `mcp serve`) that read the assembled registry. The DAG sink: depends on everything, nothing depends on it. Its lib keeps the name `aoide` so no caller changes. | `crates/cli/` (bin/*, cli.rs, dispatch.rs, registry.rs, commands/{mod,meta,stubs,infra}.rs, guide.rs, lib.rs) | landed as `crates/cli` (Phase 9 — supersedes the Phase 6 "stays the root package" finding, see the Phase 9 status note) |
| **screen** | Read-only desktop view for agents plus pointer synthesis: `screen info` (hyprctl monitors/cursor/workspace/clients), `screen shot` (grim/slurp + JSON sidecar), `screen point` (native `zwlr_virtual_pointer_v1` synthesis, no `wlrctl`), `screen ocr` (tesseract), `screen diff` (pixel/inventory act-verification), `screen point text` (click by OCR'd word) — plus its CLI commands. Carries the workspace's only heavy deps (`wayland-client`, `wayland-protocols-wlr`, `image`); `lyra`-only, core never depends on it. | `conduct`'s former screen/pointer modules | landed (P-A1) |
| **lyra** | The paint app crate: bin `lyra`, package `aoide-lyra` — the second composition root over the same domain-crate handler code `cli` assembles, its own independent `Registry`/dispatcher/golden command-path snapshot. Owns the self-ricing loop, `screen`, `herald`, `shellbridge`, and `quickshell`; deliberately never conducting, the graph, A2A, nodes, or the daemon — those stay core identity in `cli`. The one binary allowed to be nix-dependent. | *(new)* | landed (Workstream A, P-A0–A8) |

## The steward (`steward`) — detail

The headline new package. What it is and is not:

- **System management, not coding.** Its job is to keep the running Aoide
  coherent — rebuild/switch, run design/ricing passes, wire gadgets, tend songs,
  watch the session DAG. Its tools are `management` commands, not file edits in a repo.
- **Distro-agnostic by construction.** The steward manages packages, themes, and
  services through Aoide's *own* Nix-managed world via `management`'s backend
  seam — so its capabilities are identical whether it's cloned onto NixOS or a
  generic Nix-on-Arch/Debian/Fedora host. It never assumes NixOS; "self-ricing,
  agent-conducting, anywhere Nix runs" is the steward's operating envelope.
- **Conductor-driven.** It is invoked and steered through the conductor CLI/TUI
  (`conductor` crate) — a first-class conductable session like any other, not a
  hidden background daemon. The human stays in the loop; the steward proposes and
  acts under the same audit + gate every door obeys.
- **`canon` — memory of design primitives.** The user has a design language
  (Pantheon stele grammar, shade-glyph meters, box-drawing frames, palette-driven
  signatures, `gadgetW = 360`, light-only vision-check …). The
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
  `nixos-rebuild`/`switch`/package-install-or-upgrade code anywhere in the
  Rust binary today — by deliberate house policy (rebuild is user-gated, no
  self-updaters; see `AGENTS.md`). What host effect does exist is narrow and
  explicitly user-triggered, never autonomous: `shellbridge`'s power actions
  (`systemctl suspend`/`hibernate`/`reboot`/`poweroff`, `hyprctl dispatch
  exit`, fired only by a UI button press) and, inside the ricing surface
  itself, one env-guarded `hyprctl` call in `song::live` — none of it reaches
  `nixos-rebuild`, `switch`, or service *installation*, the shape
  `management`'s charter actually names. The env-guarded `hyprctl` call stayed a function separate
  from `song::live::geometry_keywords` so a future `management` crate could
  lift just the host-effect half out later without touching the pure
  computation. `management` is deferred indefinitely until real host-ops
  commands (rebuild/switch/service installation) actually get built —
  there's no ETA, it's not "next," it's "whenever that work exists to
  extract."
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
  agents at once. **This was called the last phase of the crate restructure at
  the time** — Phase 9 later superseded the "`cli` stays the root package"
  half of this note (the handler-distribution half was never the claim).
- **Phase 7 — build `steward` (the agent). DEFER.** New crate against
  protocol + conduct + storage + management: harness → tools → canon → audit →
  skills. The first genuinely new capability, not a carve-out. Verdict: DEFER.
  Two of its five pillars are blocked: `harness` (a native LLM turn-loop)
  would require aoide to adopt a model client it has explicitly refused to
  own — "any agent with a shell is fully capable" is the core thesis, and
  aoide wraps no LLM API; `tools`/`skills` need `management`'s host-ops commands,
  which don't exist (see above). The one real raw material — a rich
  design-decision corpus already on disk (`song/songbook/*/design/*.md`, the
  wiki design-language pages) that `canon` would formalize — isn't worth a
  crate yet, because the docs are prose, not structured/checkable records; a
  `canon show` command today would just be `cat` with a registry entry wrapped
  around it. Unblock conditions, in priority order: (1) a concrete decision
  from khoa on whether `steward` is LLM-driven itself or a command surface an
  external agent drives via `conduct`; (2) `management` actually built;
  (3) a decision to formalize canon into typed primitives.
- **Phase 8 — elevate `evals`. DEFER, strictly downstream of Phase 6 and
  Phase 7.** Move golden snapshots in; add steward-behavior, door-contract,
  and ricing/design eval suites. The blueprint's "golden snapshots
  (existing)" turned out to be exactly one 50-line test
  (`command_paths_match_the_golden_snapshot` in
  `pkgs/aoide/crates/cli/src/registry.rs`)
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
- **Phase 9 — distribute the command handlers; app shell → `crates/cli`;
  virtual workspace root. LANDED.** The Phases 2–6 restructure moved the
  domain LOGIC but left ~4,000 lines of command *handlers* in the root
  package's `src/commands/` — this phase finishes the blueprint's own target
  table by moving each handler group into its domain crate's `commands`
  module (nushell-style: a domain's CLI commands live with the domain;
  `graph`/`hooks` → conduct, `rice`/`cover`/`design` → song, `usage` →
  storage, `a2a serve`/`daemon`/`shellbridge` → server, the `a2a agent`
  commands/`adapter melete` → client, `conductor` → conductor). Shared
  prerequisites: `cmd!`/`arg!`/`flag!` + `audit_log_path` → `protocol`;
  `test_support` + `env_lock` → the new dev-only `test-support` crate;
  `[workspace.dependencies]` centralizes every version (Cargo.lock
  unchanged). Root keeps only the registry-coupled groups (`meta`, `stubs`,
  `mcp serve` — the one handler reading the assembled registry's tool count,
  the coupling the DI seam can't sever). The app shell then moved to
  `crates/cli` (package `aoide-cli`, lib still named `aoide`) and the root
  `Cargo.toml` became a VIRTUAL workspace — **this deliberately supersedes
  the Phase 6 finding that "`cli` is the root package"**: that verdict was
  about not adding a no-invariant indirection layer *while the handlers made
  the root the real monolith*; with the handlers distributed, the remaining
  shell is exactly the textbook app crate and the virtual-root layout is the
  monorepo standard. `pkgs/aoide/default.nix` builds/tests/installs from
  `buildAndTestSubdir = "crates/cli"`. Guards that prove behavior preserved:
  `schema --json` byte-identical across every phase, the golden command-path
  snapshot unchanged, full `cargo test --workspace` green. (No Phase 7/8
  code; Phase 9 numbers after them to keep their DEFER notes stable.)

## Two binaries — `aoide`/`aoided` (core) and `lyra` (paint) — LANDED (Workstream A, P-A0-A8)

The mapping above is the crate split; this is a narrower, later question it
doesn't answer on its own — how many *binaries* the crates assemble into.
**Decided: two.** Recorded here now, ahead of any crate move, so the
reasoning is fixed before code follows it (Workstream A, phase P-A0 of
`functional-singing-boole.md`).

Core keeps `aoide`/`aoided` over `protocol` · `storage` · `client` ·
`conduct` · `server` · `secrets` · `conductor` · `upkeep` · `cli`. A second binary —
**`lyra`** (crate `aoide-lyra`, `crates/lyra`) — owns the
rice/draft/mode/cover/livery/quickshell/screen/shellbridge/herald command
surface: everything that paints, or that only a desktop needs.

- **Naming.** Muse names (`Aoide`, `Melete`, `Mneme`) name SYSTEMS, not
  binaries; a binary living inside a system takes an INSTRUMENT name — the
  desktop is the instrument the muse plays. Considered and rejected:
  `terpsichore` (spends a muse name on a binary — the wrong tier — and at
  12 characters is a poor CLI command prefix); `aoide-rice` (mislabels the
  ~40% of the surface — screen, shellbridge, herald, quickshell — that
  isn't rice at all).
- **`conductor` stays core.** It is the messaging/orchestration surface a
  headless box needs most, not a painting tool: pure Rust (ratatui), no
  system-closure weight, and conducting orchestration is Aoide's core
  identity — see "What Aoide is" above ("she conducts every agent on
  it"). It ships in `aoide`/`aoided`, never `lyra`.
- **Charter exceptions (deliberate — recorded here as the smudge they
  are).** `shellbridge.rs` and `herald.rs` stay as FILES in `conduct` —
  only their REGISTRY lines (the CLI commands) move to `lyra` — because both
  are entangled with core: `permit.rs:413` publishes summons through
  `herald`, and `conductor/ui.rs:508` reads the socket path `shellbridge`
  owns. `storage::takes` and `storage::mode` stay in `storage` for the
  same shape of reason: zero dependency weight, and `mode` is read by
  `shellbridge`, which itself stays core-crate-resident. None of these
  four are architecture; they are named, deliberate exceptions to the
  split's own boundary, not oversights.
- **Not `management`.** This split is unrelated to the deferred
  `management` carve-out (see its status note above, under Phase 5) —
  that crate is still deferred indefinitely, still waiting on real
  host-ops commands to exist before there is anything to extract. The
  two-binary split neither touches it nor resolves it.
- **Cordis correspondence.** In Cordis's terms (CONTRACTS.md §0): a crate
  is a package/plugin; an app crate — `cli` (core) and `lyra` (paint),
  since this split — is a dsh-style BUNDLE, the ordered composition
  performed at boot; the assembled `Registry` is the plugin tree. An app
  crate's
  explicit `register()` list is the bundle's PROFILE, not a violation of
  "no registry an author must edit to be seen" (CONTRACTS §0) — composing
  a bundle at its root is how Cordis composes too. Reference:
  `github.com/deepseek-ai/deepseek-harness`, the repo behind the Cordis
  paper CONTRACTS §0 already cites.
- **Nix-independence.** Core (`aoide`/`aoided`) builds with `cargo` and
  runs on any Linux: no nix shell-outs, no NixOS assumption. `song`'s
  `widgets.rs` `nix eval` call lives in `lyra` now — only `lyra` may be
  nix-dependent. This is the load-bearing half of "What Aoide is" above,
  made structural rather than aspirational.
- **A third artifact, not a third binary — `aoide-static`.** Core has no C
  dependencies (HTTP shells out to `curl` rather than linking
  openssl/rustls), which makes it staticable: `pkgs/aoide/flake.nix`
  exposes `aoide-static`, the same `aoide`/`aoided` pair cross-built
  against musl with `+crt-static`
  (`pkgs.pkgsStatic.callPackage ./default.nix { paint = false; }` —
  nixpkgs retargets rustc, cargo, and the linker together, so proc-macro
  crates need no hand-holding to still build native). It carries no `rice`
  output and skips its own test phase — the dynamic `pkg-aoide` check
  already runs the identical suite over identical sources, so this
  variant's only job is proving core links and runs with zero dynamic
  interpreter and no reference to a nix store. Reachable as
  `packages.<system>.aoide-static` and the `pkg-aoide-static` check, never
  as `packages.default` — NixOS hosts keep the dynamic multi-output build
  `aoide.lyra.enable` depends on.
- **The shell SOURCE is a package, not a crate — `pkgs/lyra-shell`.** The QML
  tree, the resolved icons and the preview fixture sets a shell is generated
  from live in the ROOT repo (`pkgs/lyra-shell/{qml,icons,preview}`) and ship
  verbatim as `${pkgs.lyra-shell}/share/lyra/{qml,icons,preview}`. Plain
  source: it runs nothing and names no config, so it is not a shell — the
  directory a shell runs is `aoide.quickshell.config`, and what fills it is a
  song's business. Core cannot read it (this flake is rooted at `pkgs/aoide`
  and never walks the root repo), and `lyra` reads the CHECKOUT's copy rather
  than the store path, naming it once as
  `aoide_storage::fs::LYRA_SHELL_SRC`. Published by the root flake as
  `packages.<system>.lyra-shell` and consumed as `pkgs.lyra-shell` through
  the same `lib/pkgs.nix` overlay as every other root package. **Never
  `lyra`:** that attribute is an unrelated nixpkgs package, and in THIS
  document `packages.<system>.lyra` is core's rice output — one name, one
  meaning, per flake.

- **The shipped songbook is a package — `pkgs/lyra-songbook`.** The score a
  machine with no checkout falls back to: `share/lyra/songbook/` holding the song
  folders this instance ships plus baked `manifest.json`/`registry.json` and —
  for a host that selected songs — `builtin.json`; beside it
  `share/lyra/nix/` (the OFFLINE generator: `manifest.nix`, copies of
  `lib/songbook.nix`/`lib/song.nix`, and the locked nixpkgs `lib/`) and
  `share/lyra/aoide-options.json` (`lib/options.nix`'s output, read by
  `lyra onboard`). `aoide_storage::fs::song_templates_dir` finds the songbook
  through `AOIDE_SONG_TEMPLATES` or the exe-sibling path, and `fs::lyra_share_dir`
  is that path's parent; only `aoide-song`/`aoide-lyra` read either. THREE
  arguments: `songs` (which folders ship) and `builtin` (the selection, written to
  `builtin.json`), both defaulted for this flake's own
  `packages.<system>.lyra-songbook` (every discovered song) and both overridden
  per host by the `lyra` lane, plus `aoideOptions` (the option doc list, which
  `callPackage` cannot derive — each build context names it through
  `lib/pkgs.nix`'s `extra`). **What it copies is a host contract,**
  not tidiness: each shipped song enters the derivation as its OWN store path
  and the baked manifest/registry are over the shipped set only, because this
  package's path is a STRING in a host's session variables and activation — so
  anything that rehashes it moves that host's toplevel drvPath. While the build
  read the songbook DIRECTORY, every song in the repo could move a host that
  builds one song in.

## Flake outputs — the export surface

`pkgs/aoide/flake.nix` is the seam a consumer builds against — this
repo's root flake, and any stranger's flake alike:

```
pkgs/aoide/flake.nix
├── packages.<sys>
│   ├── default        = aoide
│   ├── aoide            multi-output derivation
│   │     .out  -> bin/aoide, bin/aoided
│   │     .rice -> bin/lyra
│   ├── aoide-static     musl core pair
│   └── lyra             = aoide.rice
├── apps.<sys>.{aoide,aoided}
├── overlays.default      final: _prev: { aoide = …; }
├── nixosModules.default  core option contract + `aoided.service` + overlay
├── checks.<sys>.default    = the package build
└── devShells.<sys>.default
```

The stability contract — what a consumer may rely on staying true:

- `packages.<sys>.default` is `aoide`, never `aoide-static`.
- `packages.<sys>.aoide` keeps output NAMES `out` and `rice`. The
  names are the contract; the count is not.
- `packages.<sys>.aoide-static` is the core pair only: no `rice`
  output, no test phase.
- `packages.<sys>.lyra` is the `rice` output of `packages.<sys>.aoide`
  — same derivation, never a second build.
- `overlays.default` sets exactly one attribute, `aoide`, to the same
  derivation `packages.<sys>.aoide` names. One build, never two.
- `nixosModules.default` declares the core `aoide.*` option contract
  (`enable`, `root`, `checkout`, `auditLog`, `terminal`, `user`,
  `sessionTarget`), applies `overlays.default`, and owns the
  `aoided.service` unit itself: its tmpfiles rules and its core session
  variables (`AOIDE_TERMINAL`, `AOIDE_ROOT`, `AOIDE_FLAKE_ROOT`) — one
  import carries all three. `sessionTarget` (default `default.target`)
  is the seam a paint-dependent anchor enters through: the unit's own
  `wantedBy`/`after` read it, never a lane's option directly, and the
  anchor decides only when the unit starts — no `partOf`, so a desktop
  that dies leaves the daemon and its doors running.
  `modules/nucleus/options.nix` is this repo's own consumer of the CORE option
  contract (it declares none of `enable`/`root`/`checkout`/`auditLog`/
  `terminal`/`user`/`sessionTarget` itself): the module that carries
  `overlays.default` is the core one (`pkgs/aoide/module`'s
  `nixosModules.default`), and it is imported by the nucleus lane
  (`lib/aoideos.nix`'s `nucleusModule`), which is also what `tests/vm-boot.nix`
  takes — so the constructor and the VM read one import site rather than each
  carrying their own copy of the injection lambda. And
  `modules/dendrites/quickshell.nix` sets `aoide.sessionTarget` to
  `graphical-session.target` when it has a shell config to run. Editing
  `module/` never rebuilds the binaries: `pkgs/aoide/default.nix`'s
  `src` filter drops the top-level `module/` directory, so the
  package's store path moves only on a crate-tree change.
- `checks`, `devShells` and `apps` are development surfaces, not a
  consumer contract.

**Consumer proof.** A stranger's flake runs `aoided` with no AoideOS
import — no `modules/` import, no `inputs.quickshell`/`hyprland`/`nvf`
in specialArgs; the five-line shape lives in `pkgs/aoide/module/README.md`.
Acceptance evidence (`system.build.toplevel.drvPath`, `aoide.root`,
`systemd.user.services.aoided.wantedBy`, `pkgs.aoide.outPath`,
`systemd.user.tmpfiles.rules`) is recorded in
`docs/Aoide-Wiki/ingest/log.md`.

Deliberately not exported here, so none of these get added on
assumption:

- `homeModules.*` — no consumer exists; the one named consumer beyond
  this repo (Osaka, dxflake) is NixOS.
- `nixosModules.<per-unit>` — one name suffices while every unit
  self-gates; splitting stays reversible.
- an `aoide.package` option — nothing overrides the package, and
  `overlays.default` already answers that.
- the door/secrets/usage/pair-watch units — still AoideOS-side.
- `songbookManifest` / `aoideOptions` from the core flake — both need
  `song/` and `modules/`, which this flake never walks.
- a `lyra` flake, or any Quickshell/rice export — `lyra` is paint, per
  root `AGENTS.md`; the shell's own source package is the ROOT flake's
  `packages.<system>.lyra-shell`, never anything exported here.
- `lib/walk.nix` did NOT stay: the songbook's discovery is
  `lib/songbook.nix`'s typed `discover` now — the same file that validates a
  host's song selection and generates the manifest — so there is nothing left
  for a second walker to do.

## The root flake — AoideOS's export surface

`flake.nix` (this repo) is the seam a consumer builds AoideOS against, and it is
consumed ONLY through these outputs — never through a path into this tree, which
is the reach-in the table below retires. `tests/consumer/` is the fixture that
proves it: a stranger's flake selecting the nucleus lane, the shell, the songs and
the theme, whose only inputs besides nixpkgs are home-manager and Aoide, with one
host performing from its own songbook and one performing Aoide's `sonata`.

```
flake.nix
├── nixosModules.nucleus      the nucleus lane: the ONE module that closes over
│                             Aoide's own flake inputs and hands them to every
│                             lane as `aoideInputs`
├── nixosModules.<name>       ONE per catalogue name (`modules/default.nix`),
│                             each that capability's PATH — `import`ing it
│                             yields the lane record `{ body; nixos; }`, which
│                             is exactly what a catalogue value is
├── lib.composition           the constructor, a function of `{ lib }`
├── lib.livery                the venue-recolour resolver, a function of `{ lib }`
├── lib.songbook              discovery, selection and the songs' module list,
│                             a function of `{ lib, songbook ? … }`
├── lib.catalogue             the whole catalogue as DATA — the bulk form of
│                             `nixosModules.<name>`
├── songbookRoot              Aoide's own songbook DIRECTORY (`song/songbook`),
│                             the value `lib/aoideos.nix` builds this flake's
│                             hosts from — a PATH, not a function
└── overlays.default          every `pkgs/<name>` this tree discovers plus
                              `aoide` (never `lyra`: nixpkgs owns that name)
```

What a consumer's flake writes, in the shapes the outputs are:

| consumer's reach-in (before) | replacement export | produced by |
|---|---|---|
| `inputs.aoide + "/modules/default.nix"` | `nixosModules.nucleus` + `nixosModules.<name>` per catalogue entry; `lib.catalogue` for bulk use | S10 |
| five `song/songbook/*/rice.nix` | `lib.songbook.{discover,selectionModule,songModules}` passed to `lib.composition`'s `selectionModules` / `extraModulesFor`, over `songbookRoot` when the host performs Aoide's songs | S8 (exported S10) |
| an `import` of `lib/pkgs.nix` for the overlay | `overlays.default` | S10 |
| `inputs.aoide.packages.<sys>.default` | `packages.<sys>.aoide` (and `.lyra`, `.lyra-shell`, `.lyra-songbook`) | exists / S3 |
| `import … "/lib/livery.nix"` `.resolve` | `lib.livery.resolve` | S10 |
| `inputs.aoide = inputs.aoide.inputs.aoide` | none: `nixosModules.nucleus` closes over Aoide's own inputs | S10 |
| a private copy of the composition library | `lib.composition` (habit's, re-exported) | S1 (exported S10) |
| six `aoide.facets.*` reads | the facts `aoide.{quickshell,lyra,stylix,compositor,greeter}.enable` | S2/S5 |

The stability contract:

- `nixosModules.nucleus` is a module a NixOS host imports; `nixosModules.<name>`
  is a PATH a consumer's own `registry.catalogue` takes unchanged, because a
  catalogue's values ARE paths (habit's composition imports them).
- `lib.*` are their files' own functions, still UNAPPLIED: a consumer applies
  them with ITS lib (and the songbook its host performs from), so selection runs
  on the consumer's evaluation and not on this flake's.
- `songbookRoot` is a PATH, the same value this flake's own hosts are built
  from, a top-level output beside `songbookManifest`. `lib.songbook`'s own
  default is not a substitute: a consumer needs the directory itself to hand the
  lyra lane.
- `overlays.default` is the base package set. A lane's replacement of a name the
  walker also supplies stands only for a name listed in `lib/pkgs.nix`'s
  `intentionalOverrides`; any other replacement is an evaluation error naming
  the package.

### `aoideInputs` — the three doors, and which moment each answers

A lane reads Aoide's own flake inputs (quickshell, nvf, stylix, hyprland, the
core) as the module argument `aoideInputs`, and a consumer threads nothing. The
value is closed over in `nixosModules.nucleus` (`lib/aoideos.nix`). It arrives
through three doors because the module system asks for an argument in three
different moments, and a door that answers the wrong moment is a broken
evaluation, not a style choice:

- **inside `config`** — `_module.args.aoideInputs`, set by that one module.
  `_module.args` is `raw`, so a second definition is a conflict: nucleus is the
  only site.
- **while an `imports` LIST is resolved** — by nucleus importing, BY VALUE, the
  two upstream modules whose lanes used to name them (the core's module, which
  was `modules/nucleus/options.nix`'s import, and stylix's, which was the stylix
  lane's) and by `lib/options.nix` importing the core's for its bare doc-list
  evaluation. An `imports` list is what the module list is built from, and a
  `_module.args` value is read out of `config`, which is computed FROM that
  list — reading one there is an infinite recursion, which nixpkgs reports as
  "argument `x` is not externally provided, so querying `_module.args` instead,
  requiring `config`". A lexical value in the file that does the importing costs
  no argument at all.
- **inside a Home Manager evaluation** — `home-manager.extraSpecialArgs.aoideInputs`,
  guarded on `options ? home-manager` (asked inside `config`, for the reason
  above). Home Manager runs its own module system, where `extraSpecialArgs` IS
  its specialArgs — external, therefore available while its own module
  collection runs, which is what the neovim lane's
  `imports = [ aoideInputs.nvf… ]` needs. An outer `_module.args` does not cross
  into that evaluation; `home-manager.sharedModules` puts the name back into
  `config` and meets the same wall as the `imports` door.

### Songs, from a consumer's side

A consumer performs from one songbook DIRECTORY per host: Aoide's own, exported
as `songbookRoot`, or one in the CONSUMER's tree. `lib.songbook` takes the
directory (`songbook ? …`); the consumer passes the same value, once, into the
songs hook it writes for `extraModulesFor` (`_module.args.songbook`, beside
`song` and `borrow`); and the lane that paints the built-in songs and
`pkgs/lyra-songbook` take it as an argument instead of naming a path in this
repo. A host that performs Aoide's `sonata` therefore writes
`songbook = aoide.songbookRoot` in both places and `song.declared = "sonata"` on
its record — never `inputs.aoide + "/song/songbook/…"`. A host says what it
performs (`song.declared`) and what it merely builds in (`song.available`), with
the borrow closure and the machine-owned songbook unchanged from §5.

Discovery, selection and `borrow` all read that one directory, so a host's songs
come from Aoide's songbook or from the consumer's, not from both: there is no
union of two songbooks, and a `borrow` across them has nothing to resolve.

**Stylix.** A consumer must NOT import the stylix module itself once it selects
the `stylix` lane: the lane's module comes from Aoide's own inputs, and two
copies through different input values do not deduplicate.

**Deliberately not exported:** `lib/options.nix` (it reads this tree's
`modules/`), the aggregations and override records (a registry's groupings and
fixes are the consumer's own data), this repo's `hosts/` and `users/` (machines
and people are a machine's business, and `tests/consumer/` writes its own), and
anything Quickshell-shaped (paint ships as `packages.<sys>.lyra-*`).

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
  host-ops commands actually get built and `management` is carved out, plus how
  the backend is detected at runtime (are we on NixOS, or generic-Nix?).
