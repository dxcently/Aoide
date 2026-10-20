---
type: concept
created: 2026-07-26
updated: 2026-08-29
tags: [aoide, architecture, desktop, strillix, pipeline]
source: "[[references/AOIDE-HANDOFF]]"
---

# Full Architecture — the Whole Body

The subsystems, their boundaries, and the inputs and outputs that connect them.
Each subsystem has a dedicated page; read this one after [[Overview]] for the
seams, then drill into the linked pages for detail.

The organizing thesis (see [[Rice-Vocabulary]]): architecture is frozen music.
The **frozen half** is the nix layer — the score. The **performed half** is the
running desktop — the performance. **theme** is the one seam where they meet.

## Status — running live on yomi-strix

`nix flake check` is green, both packages build, the stack boots
headless in the `vm-boot` QEMU check, and yomi-strix runs it as its daily
graphical session. Live now: a greeter (ly, or greetd with the declared rice's
Quickshell face) → Hyprland → Quickshell (bar, dock,
gadgets, launcher, notifications, wallpaper — all one process, see
[[Quickshell]]), NetworkManager, zram, the Strix Halo amdgpu params, the aoide
binaries + git on PATH, flakes enabled; the previous [[dxflake]] generation
stays in the systemd-boot menu as the rollback. Every subsystem below is
marked on one of three rungs:

- **Implemented** — real code paths: the flake/constructor/checks layer, the
  option contract, the theme plumbing, the CLI trunk + MCP façade, the daemon
  and bridge skeletons, the paint dendrites, rice replay, the launcher, the
  gadget dock.
- **Stubbed** — the mutating CLI commands (`rice declare/transpose`,
  `content *`, `make`, `update`) parse, audit, and exit 64 with a
  structured not-implemented payload; only the live action is deferred.
  (`rice lint`/`rice stage`/`rice compose`/the `rice draft` group are real —
  see [[Self-Ricing]]. There is no `rice gen`: cut outright, not stubbed —
  see [[aoide-cli]].)
- **Future** — v1 theme tiers, `rice declare`/`rice transpose` (still
  stubs), the network-exposed Aoide connector.

The repo tracks a git remote (`origin`). File-level detail lives in
[[Codebase]]; this page stays at the map altitude.

## The two halves and the seam

```
        FROZEN HALF (the score)                PERFORMED HALF (the performance)
        nix eval · immutable · git             live desktop · ephemeral · running
   ┌──────────────────────────────┐        ┌──────────────────────────────────┐
   │  nucleus  dendrites           │        │  Quickshell   Hyprland            │
   │  aggregations overrides       │        │  shellbridge  terminals           │
   │  hosts    rices       │        │  notifications  widgets           │
   └──────────────┬───────────────┘        └───────────────┬──────────────────┘
                  │                                         │
                  └──────────────►  Theme   ◄──────────────┘
                                   the only seam
                        values frozen into the crystal,
                             sounded at runtime
```

Everything below is one of these two halves, or the theme seam, or the agent
control plane that drives them.

## Master map — how the subsystems connect

Top is the driver (the agent); bottom is the pixels. Every arrow is a real input
or output, not an abstraction. Boxes marked *(stub)* exist as schema + audit
trail but exit 64 today.

**Binary note.** The map below draws both binaries as one continuous picture
of the data flow, labeled `aoide` throughout for readability. Ownership
(`CONTRACTS.md` §3, [[Package-Layout]]): the AGENT INTERFACE/`aoided`/CONTENT
PIPELINE/NIX EVAL boxes are `aoide`'s (81 commands: conducting/orchestration
is aoide's identity); the RICE ENGINE, THEME, Quickshell, and shellbridge
boxes below them are `strillix`'s (48 commands, its own schema and dispatch,
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
   theme·rice/        discover→…→query          walker: modules/ +
   (lint/stage real)           │                    rices/
       │                       ▼                         │
       ▼                  index (points in           resolves
  rices/<rice>     place, never copies)          │
  rice.nix + theme.json                                 ▼
  rices/ (write-back)                          ┌─────────────┐
       │                                          │  Theme   │  aoide.theme — one source
       └───────────────────────────────────────► └──────┬──────┘  [[theme]]
                                    two fan-outs         │
                       ┌─────────────────────────────────┴───────────────┐
                       ▼ (rehearsal / live)                (recording / baked) ▼
        theme emit {stage · hyprctl · osc}      rice.nix → dendrites + [[Stylix]]
          │              │            │                               │
          ▼              ▼            ▼                               ▼
  $STRILLIX_ROOT/stage/theme.json hyprctl    terminal OSC        hyprland.conf · QML colors ·
          │              keywords   (color inject)      base16 for every nix app
          ▼
   Quickshell — ThemeState.qml watches the stage file (hot-reload)
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
| [[Agent-Interface]]  | agent commands; `aoide`/`strx schema --json`   | dispatched operations; structured `--json` results  | implemented (aoide 74 real/7 exit 64; strillix 47 real/1 exit 64) |
| [[aoided]]           | CLI+MCP operations; desktop events             | audit log (`$AOIDE_ROOT/log`); default-deny event bus   | implemented (skeleton)                     |
| [[Self-Ricing]]      | prompt/wallpaper; `rices/`; shipped standard | `rices/<rice>/`; rices append; stage    | mostly real (`transpose` exit 64) |
| [[Content-Pipeline]] | folders + manifests; Mneme API                 | in-place index; quarantine on lint fail             | stubbed (all commands exit 64)                |
| [[theme]]         | `aoide.theme` (palette + component tiers)   | `/stage/theme.json`; baked dendrites + Stylix   | implemented (v0)                           |
| [[shellbridge]]      | unix-socket commands; Hyprland IPC             | atomic JSON in `state/stage/` (conducting) + `/stage/` (rice); `hyprctl` dispatch    | implemented (accept loop live: `focuswindow`) |
| [[Quickshell]]       | `state/stage/*.json` (sessions/hooks/graph/notifications) + `/stage/*.json` (theme/mode)  | widget socket commands; rendered surfaces            | implemented (9 real surfaces)              |
| [[Hyprland]]         | baked config + `hyprctl` keywords              | IPC event/state socket                              | implemented                                |
| [[Stylix]]           | base16 synthesized from the v0 palette         | themed config for every nix app                     | implemented (stands down on owned surfaces) |

## The theme seam in detail — one source, two fan-outs, zero drift

Why preview and adopted state can never diverge: both derive from the same
`aoide.theme` values. The theme schema v0 (palette `bg/fg/accent/urgent` + component
tiers `bar`/`notif`/`window`, each field `null` → palette, with the fallback
applied **in the paint dendrites**) rides the external W3C design-tokens container
format; [[theme]] (native Rust in `crates/strillix-rice/src/theme/`; commands `lint` /
`resolve` / `emit {stage,hyprctl,osc,file}`) is the engine. See [[theme]].

```
                     aoide.theme  (palette → component, v0)
                           │  single source of truth
             ┌─────────────┴──────────────┐
             ▼ REHEARSAL (live, uncommitted) ▼ RECORDING (adopted, committed)
   theme emit                    rice.nix ──► dendrites + Stylix
   ├─ stage: $STRILLIX_ROOT/stage/theme.json          │   (values baked at nix build)
   │         (atomic write; fully resolved)  ▼
   ├─ hyprctl: keyword dispatch         every nix-manageable target
   └─ osc: terminal color inject        GTK/Qt · terminal · editors
             │                          · browser · boot  (needs rebuild)
             ▼
   Quickshell hot-reload (ThemeState.qml)
```

Rehearsal is the sketch (hot-reloads, no rebuild); recording is the truth
(durable, requires the gated rebuild). GTK/Qt surfaces need an app restart and
are declare-only.

The baked side is carried by three paint dendrites, all real:

- **quickshell** — declares nine surfaces with `owner = "quickshell"` (bar,
  notifications, launcher, osd, lockscreen, greeter, wallpaper, agentWidgets,
  sessionGraph); QML rsyncs from the store into
  `$AOIDE_ROOT/run/qml/` (default `~/.aoide/run/qml/`) via home-manager
  activation (source stays
  `pkgs/strillix-shell/qml/`; no `qml/` at the repo root);
  `ThemeState.qml` watches the stage file for the live fan-out. Eight of
  the nine carry a live QML body: `AoideBar.qml` is the bar (its own popouts
  also carry the calendar and now-playing gadgets); `AoideLauncher.qml` is
  the keyboard-driven launcher (`SUPER+SPACE`); `AoideNotifications.qml` is
  the notification daemon — stack + D-Bus server, per-card body resolved
  through `WidgetSlot` to the rice's `widgets/notifications.qml` (actions/inline-reply
  spike still pending); agentWidgets is the [[Gadget-Dock]] — `AoidePanel.qml`
  holding four core gadgets (Conductor, Terminals, Meters, Power) plus an
  opt-in Usage stele, a left-edge panel that peeks its fore-edge and opens
  fully on hot-edge hover or
  `SUPER+G`, all theme-themed; osd, lockscreen, greeter, and wallpaper
  round out the set. `sessionGraph` remains declared but has no QML body —
  the DAG is rendered via bare `aoide graph`/`aoide conductor`, not a desktop
  overlay ([[Session-Graph]]).
- **compositor** — [[Hyprland]]; the system layer holds session/portal wiring,
  the home-manager layer owns `hyprland.conf` with theme baked at build and
  live-patched via `hyprctl` during rehearsal. The greeter lane logs in
  through ly, or through greetd with the declared rice's Quickshell face.
- **stylix** — [[Stylix]]; a base16 scheme synthesized from the v0 palette,
  with colliding targets stood down on **both** the NixOS and home-manager
  layers for every Quickshell-owned surface.

The `surface-ownership` and `rice-runtime-untracked` checks that police this seam
are real flake checks.

## The rice loop — where the agent writes

The self-ricing lifecycle overlays the map above: it produces theme, stages
through the live fan-out, drafts let it iterate without committing, and only
`declare` commits through the gate. See [[Self-Ricing]]. Today `declare` and
`transpose` are exit-64 stubs; `rice lint` (native `theme::lint`), `rice
stage`, `rice compose`, and the `rice draft`/`rice mode` groups are real.
There is no `rice gen` — cut outright, not stubbed. `rice stage` refuses
with `declarative-mode-locked` while `rice mode declarative` is locked (the
default) — see [[Self-Ricing#Staging vs Declarative Mode]].

```
  strx rice compose <name> [--from <rice>]
        │   scaffolds a new rice, reading rices/ (cross-cutting +
        │   the rice's own design/) FIRST
        ▼
  rice mode stage <name>   ──►  $STRILLIX_ROOT/stage/theme.json  ──►  Quickshell hot-reload
        │              (hyprctl/OSC dispatch not yet wired into stage)
        ▼              (REHEARSAL — nothing committed)
  edit the rice's files
        ▼
  rice lint   ──fail──►  reject + rices note
        │ pass
        ▼
  rice mode draft <draft-name>   ──►  ROUTES $STRILLIX_ROOT/stage/theme.json (symlink) into
        │   rices/<rice>/drafts/<draft-name>/theme.json — forks it from
        │   the current stage if new. Every further write (rice stage, a hand-edit)
        │   lands DIRECTLY in the draft file; no save step. Not committed —
        │   durable scratch; switch with another `rice mode draft <name>`.
        ▼
  strx rice declare <name>   ◄─── User gates this step
        │
        ▼
  commit to rices/<rice>/   ──►  gated rebuild   ──►  RECORDING
        │                                     ↓ fleet-available: any host sets
        └──►  append rices/ (learnings, preferences)      aoide.rice = "<name>"
              ← the "self" in self-ricing
```

`rices/` is a rice agent's writable domain; nothing else is.

Rice replay is implemented: `aoide.rice` (nucleus option, `nullOr str`,
default `null` — naming no rice performs no rice) is the rice a host
performs, and a host SELECTS its rices (`habit.rice.declared` is what it performs,
`habit.rice.available` what it builds in to stage without a rebuild);
`lib/rices.nix` finds `rices/<rice>/` by one typed scan and
`lib/rices.nix`'s hook registers each rice as an aggregation, and habit
imports exactly the performed rice's body, so a committed rice self-registers
and is selected by name — the same discipline as a dendrite. The shipped standard is rice `"sonata"` at
`rices/sonata/` — upstream-owned and evolving, exactly like any
other upstream-owned tree (nucleus, dendrites): upstream MAY still update or
iterate on it. Every OTHER rice — anything composed via `rice compose`
under a different name — is clone-owned; upstream never touches it, an
absolute guarantee unchanged by `sonata` being both shipped and actively
iterated. The rice carries theme only; the host is the
venue — its specifics and which instruments (dendrites) are enabled.
Replay = same rice, new venue (one line in `hosts/<host>/default.nix`);
transpose = new key, same venue. The `rice-shape` check asserts the rices's
shape: no stray `.nix` outside a rice's `rice.nix`/`_widgets/`, no `../`
path literal in a rice's `.nix` text, and every rice carrying both `rice.nix`
and `theme.json`; rice shape v0 is `CONTRACTS.md §5`. Full
replay treatment: [[Rice-Vocabulary#Replay — any rice, any host]].

Changes outside `rice/` land only on a lane the User ordered, its scope named;
a new dendrite branch is additive — one file plus its one catalogue line.
Mutation policy is encoded as radial distance
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

The index points at content in place — it never copies. `rices/<rice>/design/`
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
conducting orchestration is `aoide`'s identity, painting is `strillix`'s:

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
  node registry diverge, the converge closes that divergence by driving the
  same pairing ceremony over every declared node with no verified record —
  [[Doors-and-Nodes]]), the `melete` group
  (`status`/`graph`/`call` — the
  Melete MCP client), the
  1-command `a2a` door group (`a2a serve` — the outbound client is the `node`
  group below, not a separate `a2a agent` family),
  the 10-command `node` group (`node add/remove/pull/status/hub`,
  `node allow`/`spawn`, the LAN `node discover` listener with its
  `node advertise on|off` switch, and `node list`, the
  one-glance mesh roster — cross-device node federation,
  [[Node-Federation]]; `node status --json` keeps the deep per-node row
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
  registered node, which absorbed `who`), and bare `send`/`spawn`/
  `resurrect` (bare `resurrect` also walks up to a `.aoide/project.json`
  manifest — [[Session-Graph]]), all real — registering a new
  conducted session is `conduct` or `spawn`, and jumping to a
  session's window is a library call the conductor and `shellbridge` reach
  directly, not a CLI subcommand), and `onboard` — the
  core half of installation: registers the clone as a graph project, wires the
  agent harnesses (an interactive pick over `hooks install`, or `--harness
  a,b`/`--yes` non-interactive), and prints the guide. The desktop half —
  seeding `rices/preferences.md`, linking `~/rice` to the checkout's `rice/`
  (never clobbering an existing file or symlink) and the nix half — is
  `strillix onboard`'s, run after it. Interactive
  prompts CLI-wide (picker, y/N confirms, hidden input) ride `inquire`
  behind seams in `crates/protocol/src/pick.rs`; piped/non-tty behavior is
  unchanged. Stubs (7, exit 64):
  the 5-command `content` group (`register`/`propose`/`ingest`/`query`/
  `approve`), `make`, `update`. Core is nix-independent: cargo
  build, zero nix shell-outs.
- **`strillix`** — the Strillix paint binary. `strx schema --json` holds the other
  **48 commands**: the same `guide`/`schema`/`mcp serve` trio as their
  `aoide` spellings, over strillix's own registry; the 18-command `rice` group
  (`lint`, `stage`, `compose`, the 3-command `rice draft` group, the
  4-command `rice mode` group, the 5-command `rice history` rehearsal-snapshot
  group, `rice back`, `declare`; `transpose` is the 1 stub), `wallpaper set`,
  the 3-command `theme` group (`lint`/`resolve`/`emit`
  — the native design-token engine), `element seed` (renders a rice's
  committed `elements/*/element.json` into `run/elements/`), `shellbridge`,
  the 2-command `quickshell` group (`reload` — the Quickshell IPC hot-reload
  trigger, rebuilding the whole scene from `shell.qml` in-process to pick up
  dynamically-loaded widget/shell QML the file watcher can't track — and
  `healthcheck`, the placeholder-screen lockup watchdog), `notifications push`,
  `onboard` (the nix half of installation: generates `./aoide.nix` — or
  `--out <path>` — listing every
  `aoide.*` module option, 161 today, derived live from the modules via the
  flake's `aoideOptions` output, defaults commented out with one-line
  descriptions, plus the env-knob appendix as comments, and prints the
  `imports = [ ./aoide.nix ];` line for the user's own flake — it never edits
  that flake; re-running warns and backs up the old file to `<out>.bak`, and
  a file it did not generate is refused, never overwritten), `secrets ask`
  and the 2-command `pair` group (`ask`/`show` — the pairing-ceremony's
  popup dialogs, sharing `secrets ask`'s six-box QML component), and the
  14-command `screen` group
  (capture, OCR, and synthesized-pointer control — [[Screen-Control]]). Only
  `strillix` may shell out to nix.

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
`aoide-secrets-serve`, present when `aoide-secrets` is selected, own uid
`aoide-secrets` — anchored to `multi-user.target` rather than a graphical
session.
Live state lands in two stage trees under the runtime root (`$AOIDE_ROOT`,
default `~/.aoide`), split by owner: `/stage/*.json`
(theme, mode, wallpaper — rice/paint staging) and `state/stage/*.json`
(sessions, hooks, projects, graph, notifications, pending — the [[Session-Graph]]
DAG layer's own conducting files). Beside them, the strillix lane's
activation seed publishes `/declared/theme.json`, the venue's read-only
statement of the declared rice, and `/declared/venue.json`, the slots the
venue recolours and the geometry the host sets (CONTRACTS §4). `lib/strillix.nix`'s constructor
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
│                    outputs: nixosConfigurations.<host> · hosts · inventory ·
│                    packages · nixosModules · overlays · lib · aoideOptions ·
│                    checks · devShells · formatter
├── lib/             strillix.nix (the constructor, over habit's composition) ·
│                    pkgs.nix / rices.nix (discovery) · options.nix (aoideOptions) ·
│                    checks.nix (surface-ownership · rice-runtime-untracked · rice-shape …)
├── tests/           non-cargo tests: vm-boot.nix (headless QEMU boot check) ·
│                    vm-lock-greeter.nix (graphical greeter + lock check) ·
│                    portability.nix (static-artifact check) · distrobox.md (manual)
├── modules/         the snowflake — the catalogue names what is selectable
│   ├── nucleus/     options.nix (THE paint contract) · aoided · secrets · melete-adapter
│   │                · packages.nix (aoide + git on PATH) · nix.nix (flakes on)
│   ├── dendrites/   the opt-in lanes, one catalogue entry each (bash, nh, git, kitty,
│   │                neovim, starship, mcfly, btop, yazi, fastfetch, devtools, cli,
│   │                fonts, hyprland, obsidian, melete, mneme, firefox, screenshot,
│   │                vision, audio, claude-code, clipboard, dunst, kimi-code,
│   │                networkmanager, pi-coding-agent — plus the paint lanes
│   │                quickshell · compositor · stylix · greeter · strillix) ← additive
│   ├── aggregations/  named groups a host takes in one line (base · desktop · agents · strillix)
│   └── overrides/   capability-scoped fixes (no records yet)
├── hosts/           one record per machine (yomi-strix is the real one; `_`-prefixed
│                    dirs are the templates to copy) — flags, aggregations, the
│                    aoide.rice selector, and a real
│                    hardware profile, switched live and running as the daily desktop
├── pkgs/            aoide/ (Rust workspace, two binaries — aoide/aoided core +
│                    strillix paint), strillix-shell/ (the QML skeleton), strillix-rices/
│                    (the shipped templates), see [[Package-Layout]]
├── rice/            rices/<rice>/ (per rice: rice.nix · theme.json ·
│                    palette/ · sounds/ · icons/ · widgets/ · design/) ·
│                    wallpapers/ — shared wallpaper library ·
│                    rices/learnings.md · rices/update-playbook.md
├── docs/BUILD.md    module-authoring conventions
├── CONTRACTS.md     §0 design philosophy + the versioned contracts §1–8 (note
│                    schema · dendrite shape · schema output · stage files ·
│                    rice shape · A2A door · node federation · screen capture)
└── AGENTS.md        tier-0 agent guide (aoide guide prints the same map)
```

The map above is the dev git checkout (`~/Aoide`, reached via
`$AOIDE_FLAKE_ROOT`). Every runtime tree hangs off one root instead —
`$AOIDE_ROOT` when set to an absolute path, else `~/.aoide` (the `aoide.root`
option): `/stage/`, `/declared/`, `state/` (+ `state/stage/`),
`run/qml/`, the composed
host `rices/`, and the audit `log/`, created at runtime by
systemd-tmpfiles, the quickshell lane's home-manager activation, and the
binaries themselves; on first run the binaries migrate pre-existing
`~/Aoide/{rice/stage,state,log}` into the root, each piece gated on its own
override being unset. `rices/**` in the checkout is
versioned score, legitimately walked at eval.

`hosts/` knows dendrites; dendrites never know hosts. Paint lanes read only
`aoide.theme`, `aoide.layout` and `aoide.surfaces` plus the core scalars
and the rice selection root `AGENTS.md` house rule 5 enumerates; no
module reads another module.
The coupling discipline is contractual — the flake's checks (`surface-ownership`,
`rice-runtime-untracked`, `rice-shape`, plus building both packages, plus the `vm-boot`
headless boot of the assembled stack) fail eval on violation.

## Related

- [[Overview]]
- [[Plugin-Architecture]] — the design philosophy every contract on this page is downstream of
- [[Codebase]]
- [[Package-Layout]]
- [[Snowflake-Anatomy]]
- [[Desktop-Architecture]]
- [[strillix]]
- [[theme]]
- [[Self-Ricing]]
- [[Content-Pipeline]]
- [[Agent-Interface]]
- [[Governance]]
- [[Secrets-Broker]]
- [[Rice-Vocabulary]]
