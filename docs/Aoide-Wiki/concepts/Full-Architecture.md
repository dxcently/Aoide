---
type: concept
created: 2026-07-26
updated: 2026-08-29
tags: [aoide, architecture, desktop, lyra, pipeline]
source: "[[references/AOIDE-HANDOFF]]"
---

# Full Architecture — the Whole Body

The subsystems, their boundaries, and the inputs and outputs that connect them.
Each subsystem has a dedicated page; read this one after [[Overview]] for the
seams, then drill into the linked pages for detail.

The organizing thesis (see [[Song-Vocabulary]]): architecture is frozen music.
The **frozen half** is the nix layer — the score. The **performed half** is the
running desktop — the performance. **livery** is the one seam where they meet.

## Status — running live on yomi-strix

`nix flake check` is green, both packages build, the stack boots
headless in the `vm-boot` QEMU check, and yomi-strix runs it as its daily
graphical session. Live now: greetd → Hyprland → Quickshell (bar, dock,
gadgets, launcher, notifications, wallpaper — all one process, see
[[Quickshell]]), NetworkManager, zram, the Strix Halo amdgpu params, the aoide
binaries + git on PATH, flakes enabled; the previous [[dxflake]] generation
stays in the systemd-boot menu as the rollback. Every subsystem below is
marked on one of three rungs:

- **Implemented** — real code paths: the flake/walker/checks layer, the option
  contract, the livery plumbing, the CLI trunk + MCP façade, the daemon and
  bridge skeletons, the three facets, song replay, the launcher, the gadget
  dock.
- **Stubbed** — the mutating CLI commands (`rice declare/transpose`,
  `content *`, `make`, `update`) parse, audit, and exit 64 with a
  structured not-implemented payload; only the live action is deferred.
  (`rice lint`/`rice stage`/`rice compose`/the `rice draft` group are real —
  see [[Self-Ricing]]. There is no `rice gen`: cut outright, not stubbed —
  see [[aoide-cli]].)
- **Future** — v1 livery tiers, `rice declare`/`rice transpose` (still
  stubs), the network-exposed Aoide connector.

The repo tracks a git remote (`origin`). File-level detail lives in
[[Codebase]]; this page stays at the map altitude.

## The two halves and the seam

```
        FROZEN HALF (the score)                PERFORMED HALF (the performance)
        nix eval · immutable · git             live desktop · ephemeral · running
   ┌──────────────────────────────┐        ┌──────────────────────────────────┐
   │  nucleus  dendrites           │        │  Quickshell   Hyprland            │
   │  facets                       │        │  shellbridge  terminals           │
   │  hosts    song/songbook       │        │  notifications  widgets           │
   └──────────────┬───────────────┘        └───────────────┬──────────────────┘
                  │                                         │
                  └──────────────►  Livery   ◄──────────────┘
                                   the only seam
                        values frozen into the crystal,
                             sounded at runtime
```

Everything below is one of these two halves, or the livery seam, or the agent
control plane that drives them.

## Master map — how the subsystems connect

Top is the driver (the agent); bottom is the pixels. Every arrow is a real input
or output, not an abstraction. Boxes marked *(stub)* exist as schema + audit
trail but exit 64 today.

**Binary note.** The map below draws both binaries as one continuous picture
of the data flow, labeled `aoide` throughout for readability. Ownership
(`CONTRACTS.md` §3, [[Package-Layout]]): the AGENT INTERFACE/`aoided`/CONTENT
PIPELINE/NIX EVAL boxes are `aoide`'s (81 commands: conducting/orchestration
is aoide's identity); the RICE ENGINE, LIVERY, Quickshell, and shellbridge
boxes below them are `lyra`'s (48 commands, its own schema and dispatch,
routed through the desktop, not through `aoided`'s CLI trunk).

```
                          ┌───────────────────────────┐
                          │           AGENT           │  claude CLI · melete · any shell
                          └─────────────┬─────────────┘
        commands / queries              │            ▲   events (via per-agent adapters)
                                        ▼            │
   ┌─────────────────────────────────────────────────────────────┐
   │  AGENT INTERFACE            aoide <cmd>   ·   aoide mcp serve │   [[Agent-Interface]]
   │  ONE schema (aoide schema --json) ──► CLI trunk + MCP façade  │   81 commands · exit 0/1/2/64
   └───────────────────────────────┬─────────────────────────────┘
                                    ▼
   ┌─────────────────────────────────────────────────────────────┐
   │  aoided  —  policy · single audit log ($AOIDE_ROOT/log)      │   [[aoided]] · [[Governance]]
   │            default-deny event bus (per event class)           │
   │            REBUILD GATE  ◄───────────────  User admits        │
   └───┬───────────────────────┬────────────────────────┬────────┘
       ▼                       ▼                         ▼
  RICE ENGINE            CONTENT PIPELINE (stub)   NIX EVAL + REBUILD
  [[Self-Ricing]]        [[Content-Pipeline]]      [[Snowflake-Anatomy]]
   livery·song/        discover→…→query          walker: modules/ +
   (lint/stage real)           │                    song/songbook/
       │                       ▼                         │
       ▼                  index (points in           resolves
  song/songbook/<song>     place, never copies)          │
  rice.nix + livery.json                                 ▼
  songbook/ (write-back)                          ┌─────────────┐
       │                                          │  Livery   │  aoide.livery — one source
       └───────────────────────────────────────► └──────┬──────┘  [[livery]]
                                    two fan-outs         │
                       ┌─────────────────────────────────┴───────────────┐
                       ▼ (rehearsal / live)                (recording / baked) ▼
        livery emit {stage · hyprctl · osc}      rice.nix → facets + [[Stylix]]
          │              │            │                               │
          ▼              ▼            ▼                               ▼
  song/stage/livery.json hyprctl    terminal OSC        hyprland.conf · QML colors ·
          │              keywords   (color inject)      base16 for every nix app
          ▼
   Quickshell — LiveryState.qml watches the stage file (hot-reload)
   [[Quickshell]]
      │  ▲
      │  └── reads state/stage/{sessions,hooks,graph}.json (roster + DAG surfaces)
      └──► widget click → shellbridge socket → hyprctl dispatch focuswindow
                              │
                    ┌─────────┴──────┐
                    │  shellbridge   │ ◄── Hyprland IPC socket ── [[Hyprland]]
                    │ [[shellbridge]] │
                    └────────────────┘
```

## Subsystem I/O — inputs, outputs, and status at a glance

Each row is one subsystem: what flows in, what flows out, and where it stands on
the implemented/stubbed ladder.

| Subsystem            | Inputs                                         | Outputs                                             | Status                                     |
| -------------------- | ---------------------------------------------- | --------------------------------------------------- | ------------------------------------------ |
| [[Agent-Interface]]  | agent commands; `aoide`/`lyra schema --json`   | dispatched operations; structured `--json` results  | implemented (aoide 74 real/7 exit 64; lyra 47 real/1 exit 64) |
| [[aoided]]           | CLI+MCP operations; desktop events             | audit log (`$AOIDE_ROOT/log`); default-deny event bus   | implemented (skeleton)                     |
| [[Self-Ricing]]      | prompt/wallpaper; `songbook/`; shipped standard | `song/songbook/<song>/`; songbook append; stage    | mostly real (`transpose` exit 64) |
| [[Content-Pipeline]] | folders + manifests; Mneme API                 | in-place index; quarantine on lint fail             | stubbed (all commands exit 64)                |
| [[livery]]         | `aoide.livery` (palette + component tiers)   | `song/stage/livery.json`; baked facets + Stylix   | implemented (v0)                           |
| [[shellbridge]]      | unix-socket commands; Hyprland IPC             | atomic JSON in `state/stage/` (conducting) + `song/stage/` (rice); `hyprctl` dispatch    | implemented (accept loop live: `focuswindow`) |
| [[Quickshell]]       | `state/stage/*.json` (sessions/hooks/graph/herald) + `song/stage/*.json` (livery/mode)  | widget socket commands; rendered surfaces            | implemented (9 real surfaces)              |
| [[Hyprland]]         | baked config + `hyprctl` keywords              | IPC event/state socket                              | implemented (greetd stubbed)               |
| [[Stylix]]           | base16 synthesized from the v0 palette         | themed config for every nix app                     | implemented (stands down on owned surfaces) |

## The livery seam in detail — one source, two fan-outs, zero drift

Why preview and adopted state can never diverge: both derive from the same
`aoide.livery` values. The livery schema v0 (palette `bg/fg/accent/urgent` + component
tiers `bar`/`notif`/`window`, each field `null` → palette, with the fallback
applied **in the facets**) rides the external W3C design-tokens container
format; [[livery]] (native Rust in `crates/song/src/livery/`; commands `lint` /
`resolve` / `emit {stage,hyprctl,osc,file}`) is the engine. See [[livery]].

```
                     aoide.livery  (palette → component, v0)
                           │  single source of truth
             ┌─────────────┴──────────────┐
             ▼ REHEARSAL (live, uncommitted) ▼ RECORDING (adopted, committed)
   livery emit                    rice.nix ──► facets + Stylix
   ├─ stage: song/stage/livery.json          │   (values baked at nix build)
   │         (atomic write; fully resolved)  ▼
   ├─ hyprctl: keyword dispatch         every nix-manageable target
   └─ osc: terminal color inject        GTK/Qt · terminal · editors
             │                          · browser · boot  (needs rebuild)
             ▼
   Quickshell hot-reload (LiveryState.qml)
```

Rehearsal is the sketch (hot-reloads, no rebuild); recording is the truth
(durable, requires the gated rebuild). GTK/Qt surfaces need an app restart and
are declare-only.

The baked side is carried by the three facets, all real:

- **quickshell** — declares nine surfaces with `owner = "quickshell"` (bar,
  notifications, launcher, osd, lockscreen, greeter, wallpaper, agentWidgets,
  sessionGraph); QML rsyncs from the store into
  `$AOIDE_ROOT/run/qml/` (default `~/.aoide/run/qml/`) via home-manager
  activation (source stays
  `modules/facets/quickshell/qml/`; no `qml/` at the repo root);
  `LiveryState.qml` watches the stage file for the live fan-out. Eight of
  the nine carry a live QML body: `AoideBar.qml` is the bar (its own popouts
  also carry the calendar and now-playing gadgets); `AoideLauncher.qml` is
  the keyboard-driven launcher (`SUPER+SPACE`); `AoideNotifications.qml` is
  the notification daemon — stack + D-Bus server, per-card body resolved
  through `WidgetSlot` to the song's `widgets/notifications.qml` (actions/inline-reply
  spike still pending); agentWidgets is the [[Gadget-Dock]] — `AoidePanel.qml`
  holding four core gadgets (Conductor, Terminals, Meters, Power) plus an
  opt-in Usage stele, a left-edge panel that peeks its fore-edge and opens
  fully on hot-edge hover or
  `SUPER+G`, all livery-themed; osd, lockscreen, greeter, and wallpaper
  round out the set. `sessionGraph` remains declared but has no QML body —
  the DAG is rendered via bare `aoide graph`/`aoide conductor`, not a desktop
  overlay ([[Session-Graph]]).
- **compositor** — [[Hyprland]]; the system layer holds session/portal wiring,
  the home-manager layer owns `hyprland.conf` with livery baked at build and
  live-patched via `hyprctl` during rehearsal; greetd is stubbed.
- **stylix** — [[Stylix]]; a base16 scheme synthesized from the v0 palette,
  with colliding targets stood down on **both** the NixOS and home-manager
  layers for every Quickshell-owned surface.

The `surface-ownership` and `no-song-read` checks that police this seam are
real flake checks.

## The rice loop — where the agent writes

The self-ricing lifecycle overlays the map above: it produces livery, stages
through the live fan-out, drafts let it iterate without committing, and only
`declare` commits through the gate. See [[Self-Ricing]]. Today `declare` and
`transpose` are exit-64 stubs; `rice lint` (native `livery::lint`), `rice
stage`, `rice compose`, and the `rice draft`/`rice mode` groups are real.
There is no `rice gen` — cut outright, not stubbed. `rice stage` refuses
with `declarative-mode-locked` while `rice mode declarative` is locked (the
default) — see [[Self-Ricing#Staging vs Declarative Mode]].

```
  lyra rice compose <name> [--from <song>]
        │   scaffolds a new song, reading song/songbook/ (cross-cutting +
        │   the song's own design/) FIRST
        ▼
  rice mode stage <name>   ──►  song/stage/livery.json  ──►  Quickshell hot-reload
        │              (hyprctl/OSC dispatch not yet wired into stage)
        ▼              (REHEARSAL — nothing committed)
  edit the song's files
        ▼
  rice lint   ──fail──►  reject + songbook note
        │ pass
        ▼
  rice mode draft <draft-name>   ──►  ROUTES song/stage/livery.json (symlink) into
        │   song/songbook/<song>/drafts/<draft-name>/livery.json — forks it from
        │   the current stage if new. Every further write (rice stage, a hand-edit)
        │   lands DIRECTLY in the draft file; no save step. Not committed —
        │   durable scratch; switch with another `rice mode draft <name>`.
        ▼
  lyra rice declare <name>   ◄─── User gates this step
        │
        ▼
  commit to song/songbook/<song>/   ──►  gated rebuild   ──►  RECORDING
        │                                     ↓ fleet-available: any host sets
        └──►  append songbook/ (learnings, preferences)      aoide.song = "<name>"
              ← the "self" in self-ricing
```

`song/` is the agent's only writable domain.

Song replay is implemented: `aoide.song` (nucleus option, `nullOr str`,
default `null` — naming no song performs no song) selects the song a host
performs; `lib/mkHost.nix` walks
`song/songbook/` exactly as it walks `modules/`, so a committed song
self-registers and self-gates on `config.aoide.song == "<name>"` — the same
discipline as a dendrite. The shipped standard is song `"sonata"` at
`song/songbook/sonata/` — upstream-owned and evolving, exactly like any
other upstream-owned tree (nucleus, facets): upstream MAY still update or
iterate on it. Every OTHER song — anything composed via `rice compose`
under a different name — is clone-owned; upstream never touches it, an
absolute guarantee unchanged by `sonata` being both shipped and actively
iterated. The song carries livery only; the host is the
venue — its specifics and which instruments (facets, dendrites) are enabled.
Replay = same song, new venue (one line in `hosts/<host>/default.nix`);
transpose = new key, same venue. The `song-shape` check asserts every walked
songbook file is a `rice.nix`; song shape v0 is `CONTRACTS.md §5`. Full
replay treatment: [[Song-Vocabulary#Replay — any song, any host]].

Inherited structure (nucleus, facets) changes by upstream merge only; new
dendrite branches are additive. Mutation policy is encoded as radial distance
from the nucleus — see [[Snowflake-Anatomy]] and [[Governance]].

## The content pipeline — how knowledge enters

Runs beside the rice loop off the same daemon. The approve gate is the
structural break against injection. See [[Content-Pipeline]]. All five commands
(`content register/propose/approve/ingest/query`) are schema-real, exit-64
stubs today.

```
  discover ─► propose ─► approve ─► ingest ─► lint ─► query
                        (User gate)             │
                                                └─fail─► quarantine
                                                        (never poisons the live index)
```

The index points at content in place — it never copies. `song/songbook/<song>/design/`
notes are dogfooded back through this same pipeline (pre-approved, system-owned),
so the agent can query its own past rice reasoning. Mneme is read only via its
API; the vault declares exports, Aoide admits through the same gate.

## The control plane — one gate, one log, three doors

Everything an agent can do is one command schema with three front doors that
cannot drift, funnelled through a single policy/audit surface — one
implementation with three façades, the [[Plugin-Architecture#Spatial and
temporal composability|same thesis]] as the walker and the widget slots. See
[[Agent-Interface]], [[A2A-Door]], and [[Governance]].

```
   agent ──► CLI trunk ─────┐
   agent ──► MCP façade ─────┼──►  aoided  ──►  policy · GATE · audit($AOIDE_ROOT/log)
   agent ──► A2A door ───────┘         (all generated from the same schema)
```

This is shipped code, now two binaries, per-binary schema
(`docs/architecture/PACKAGE-LAYOUT.md`, "Two binaries"; `CONTRACTS.md` §3) —
conducting orchestration is `aoide`'s identity, painting is `lyra`'s:

- **`aoide`/`aoided`** ([[aoide-cli]], [[aoided]]) — the orchestration core.
  `aoide schema --json` is its machine-readable source of truth; the stdio
  MCP façade (`aoide mcp serve --stdio`) generates its tool list from it, and
  the [[A2A-Door]]'s AgentCard is derived from the same schema — all
  one-to-one. **81 commands** — real (74): `guide`, `schema`,
  `mcp serve`, `daemon`, `events tail`, `conduct`, `conductor`, `adapter
  melete`, `identity`, `pair`/`pair reject`/`pair watch` (the whole
  pairing ceremony behind one smart verb, bare `pair` the interactive
  pending listing — [[Pairing-Ceremony]]), `mesh`/`mesh pair` (the declared
  mesh: the read reports where a `[mesh.<name>]` declaration and the live
  peer registry diverge, the converge closes that divergence by driving the
  same pairing ceremony over every declared peer with no verified record —
  [[Doors-and-Peers]]), the `melete` group
  (`status`/`graph`/`call` — the
  Melete MCP client), the
  1-command `a2a` door group (`a2a serve` — the outbound client is the `peer`
  group below, not a separate `a2a agent` family),
  the 10-command `peer` group (`peer add/remove/pull/status/hub`,
  `peer allow`/`spawn`, the LAN `peer discover` listener with its
  `peer advertise on|off` switch, and `peer list`, the
  one-glance mesh roster — cross-device peer federation,
  [[Peer-Federation]]; `peer status --json` keeps the deep per-peer row
  the roster never duplicates — the pairing ceremony that mints these
  records lives under `pair`, above), the 17-command `secrets` group
  (`serve`/`exec`/`add`/`rm`/`grant`/`revoke`/`enroll`/`put`/`set-totp`/
  `automate`/`expose`/`allow-remote-origin`/`migrate`/`pending`/`approve`/`dismiss`/`watch` — the
  socket-only credential broker under its own uid, [[Secrets-Broker]]),
  `usage`, `hooks install`, `soundcheck`, `config`/`config set` (the portable
  runtime config file, `$AOIDE_ROOT/config.toml`), the 3-command `inbox` group (`list`/`read`/`clear`, the
  durable per-host message store), the 20-command graph/session/project
  surface (the [[Session-Graph]] DAG viewer + management layer over projects
  and sessions — bare `graph`/`graph link` the read/analysis lens,
  `project add/remove/list`, the `session` family
  (`start/phase/end/hook/grant/permit/pending list|approve|deny/prune/reap` —
  `session grant` carries both the `undying` picker and the reaper `exempt`)
  plus bare `session` (the roster listing over this box's sessions and every
  registered peer, which absorbed `who`), and bare `send`/`spawn`/
  `resurrect` (bare `resurrect` also walks up to a `.aoide/project.json`
  manifest — [[Session-Graph]]), all real — registering a new
  conducted session is `conduct` or `spawn`, and jumping to a
  session's window is a library call the conductor and `shellbridge` reach
  directly, not a CLI subcommand), and `onboard` — the
  core half of installation: registers the clone as a graph project, links
  `~/song` to the checkout's `song/` (never clobbering an existing file or
  symlink), seeds `song/songbook/preferences.md` when absent, wires the
  agent harnesses (an interactive pick over `hooks install`, or `--harness
  a,b`/`--yes` non-interactive), then delegates the nix half to `lyra
  onboard` when the lyra binary resolves and prints the guide. Interactive
  prompts CLI-wide (picker, y/N confirms, hidden input) ride `inquire`
  behind seams in `crates/protocol/src/pick.rs`; piped/non-tty behavior is
  unchanged. Stubs (7, exit 64):
  the 5-command `content` group (`register`/`propose`/`ingest`/`query`/
  `approve`), `make`, `update`. Core is nix-independent: cargo
  build, zero nix shell-outs.
- **`lyra`** — the AoideOS paint binary. `lyra schema --json` holds the other
  **48 commands**: the same `guide`/`schema`/`mcp serve` trio as their
  `aoide` spellings, over lyra's own registry; the 18-command `rice` group
  (`lint`, `stage`, `compose`, the 3-command `rice draft` group, the
  4-command `rice mode` group, the 5-command `rice take` rehearsal-snapshot
  group, `rice back`, `declare`; `transpose` is the 1 stub), `cover set`,
  the 3-command `livery` group (`lint`/`resolve`/`emit`
  — the native design-token engine), `element seed` (renders a song's
  committed `elements/*/element.json` into `run/elements/`), `shellbridge`,
  the 2-command `quickshell` group (`reload` — the Quickshell IPC hot-reload
  trigger, rebuilding the whole scene from `shell.qml` in-process to pick up
  dynamically-loaded widget/facet QML the file watcher can't track — and
  `healthcheck`, the placeholder-screen lockup watchdog), `herald push`,
  `onboard` (the nix half of installation: generates `./aoide.nix` — or
  `--out <path>` — listing every
  `aoide.*` module option, 142 today, derived live from the modules via the
  flake's `aoideOptions` output, defaults commented out with one-line
  descriptions, plus the env-knob appendix as comments, and prints the
  `imports = [ ./aoide.nix ];` line for the user's own flake — it never edits
  that flake; re-running warns and backs up the old file to `<out>.bak`, and
  a file it did not generate is refused, never overwritten), `secrets ask`
  and the 2-command `pair` group (`ask`/`show` — the pairing-ceremony's
  popup dialogs, sharing `secrets ask`'s six-box QML component), and the
  14-command `screen` group
  (capture, OCR, and synthesized-pointer control — [[Screen-Control]]). Only
  `lyra` may shell out to nix.

There is no `rice gen` — cut outright (2026-08-14), not left as a stub in
either binary. Exit codes are contractual, identical in both binaries: 0 ok,
1 error, 2 usage, 64 not-implemented.

On the host, the plane runs as systemd user units, all from the nucleus:
`aoided`, `shellbridge` (socket `$XDG_RUNTIME_DIR/aoide/shellbridge.sock`),
the `aoide-graph-reap` timer (the liveness reaper, ~12s interval, sweeps
sessions a killed terminal could never mark `done`),
`aoide-melete-adapter`, and `aoide-mcp` (gated on `aoide.mcp.enable`, default
false) — plus the opt-in `aoide-a2a` ([[A2A-Door]], gated on `aoide.a2a.enable`)
and `aoide-usage` (gated on `aoide.usage.enable`) units, and the
`aoide-obsidian-register` oneshot from the shipped dendrite. The
[[Secrets-Broker]] runs separately, as its own SYSTEM (not user) service —
`aoide-secrets-serve`, gated on `aoide.secrets.enable`, own uid
`aoide-secrets` — anchored to `multi-user.target` rather than a graphical
session.
Live state lands in two stage trees under the runtime root (`$AOIDE_ROOT`,
default `~/.aoide`), split by owner: `song/stage/*.json`
(livery, mode, cover — rice/paint staging) and `state/stage/*.json`
(sessions, hooks, projects, graph, herald, pending — the [[Session-Graph]]
DAG layer's own conducting files). `lib/mkHost.nix`
injects `pkgs.aoide` by overlay from the **same**
`callPackage` paths as the flake's `packages` output, so the units and the
flake always build the same binaries, never a drifted copy.

- **Three connectors, three roles.** The [[Mneme]] connector is the vault door
  (knowledge); the [[Melete]] connector is the doer harness (jobs); the
  **Aoide connector** is a dedicated MCP connector specifically for managing
  Aoide and its components — tier 3 of the four-tier agent interface, the same
  command schema behind a network door. Aoide is **not** reached through the
  Mneme or Melete connectors. **Status:** specified, not built. Agents spawn
  `aoide mcp serve --stdio` per session today. See [[Agent-Interface]].
- Every rebuild is user-gated (polkit pattern): agent proposes, user admits,
  git records. `aoided` is propose-only; no background self-updaters — house
  policy, concrete in the unit definitions.
- One audit surface: every door (CLI, MCP, A2A) appends to the single audit log
  file (`$AOIDE_ROOT/log` — default `~/.aoide/log` — the `aoide.auditLog`
  option) — there is no per-door log.
- Trust boundary: forwarded notification text is untrusted data. The melete
  adapter forwards metadata only; an app title must never reach the agent as a
  command — a hard architectural constraint.

## Where each subsystem lives in the tree

The frozen half maps onto the repo layout as built (see [[Snowflake-Anatomy]]
for the layer anatomy, [[Codebase]] for file-level detail):

```
~/Aoide/
├── flake.nix        inputs: nixpkgs · home-manager · stylix · quickshell · hyprland
│                    outputs: nixosConfigurations.yomi-strix · packages.aoide
│                    · checks · devShells · formatter
├── lib/             walk.nix (dendritic walker) · mkHost.nix (host assembly + pkgs
│                    overlay) · checks.nix (surface-ownership · no-song-read · song-shape)
├── tests/           non-cargo tests: vm-boot.nix (headless QEMU boot check) ·
│                    portability.nix (static-artifact check) · distrobox.md (manual)
├── modules/         the snowflake — walker-discovered layers
│   ├── nucleus/     options.nix (THE contract) · aoided · shellbridge · melete-adapter
│   │                · packages.nix (aoide + git on PATH) · nix.nix (flakes on)
│   ├── dendrites/   27 opt-in features (bash, nh, git, kitty, neovim, starship,
│   │                mcfly, btop, yazi, fastfetch, devtools, cli, fonts, hyprland,
│   │                obsidian, melete, mneme, firefox, screenshot, vision, audio,
│   │                claude-code, clipboard, dunst, kimi-code, networkmanager,
│   │                pi-coding-agent) ← additive
│   └── facets/      quickshell · compositor · stylix          ← render surfaces (lyra-only)
├── hosts/           common/ + yomi-strix/ (flags + the aoide.song selector; a real
│                    hardware profile, switched live and running as the daily desktop)
├── pkgs/            aoide/ (Rust workspace, 13 crates over two binaries —
│                    aoide/aoided core + lyra paint, see [[Package-Layout]])
├── song/            songbook/sonata/ (shipped standard) — rice.nix · livery.json ·
│                    palette/ · sounds/ · icons/ · widgets/ · design/ (per song);
│                    covers/ — shared wallpaper library, referenced by rice.nix
├── docs/BUILD.md    module-authoring conventions
├── CONTRACTS.md     §0 design philosophy + the versioned contracts §1–8 (note
│                    schema · dendrite shape · schema output · stage files ·
│                    song shape · A2A door · peer federation · screen capture)
└── AGENTS.md        tier-0 agent guide (aoide guide prints the same map)
```

The map above is the dev git checkout (`~/Aoide`, reached via
`$AOIDE_FLAKE_ROOT`). Every runtime tree hangs off one root instead —
`$AOIDE_ROOT` when set to an absolute path, else `~/.aoide` (the `aoide.root`
option): `song/stage/`, `state/` (+ `state/stage/`), `run/qml/`, the composed
host `song/songbook/`, and the audit `log/`, created at runtime by
systemd-tmpfiles, the quickshell facet's home-manager activation, and the
binaries themselves; on first run the binaries migrate pre-existing
`~/Aoide/{song/stage,state,log}` into the root, each piece gated on its own
override being unset. `song/songbook/**` in the checkout is
versioned score, legitimately walked at eval.

`hosts/` knows dendrites; dendrites never know hosts. Facets read only
`aoide.livery` and `aoide.arrangement` (and declare `aoide.surfaces`); no
module reads another module.
The coupling discipline is contractual — the flake's checks (`surface-ownership`,
`no-song-read`, `song-shape`, plus building both packages, plus the `vm-boot`
headless boot of the assembled stack) fail eval on violation.

## Related

- [[Overview]]
- [[Plugin-Architecture]] — the design philosophy every contract on this page is downstream of
- [[Codebase]]
- [[Package-Layout]]
- [[Snowflake-Anatomy]]
- [[Desktop-Architecture]]
- [[lyra]]
- [[livery]]
- [[Self-Ricing]]
- [[Content-Pipeline]]
- [[Agent-Interface]]
- [[Governance]]
- [[Secrets-Broker]]
- [[Song-Vocabulary]]
