---
type: concept
created: 2026-07-26
updated: 2026-08-01
tags: [aoide, architecture, desktop, drachma, pipeline]
source: "[[references/AOIDE-HANDOFF]]"
---

# Full Architecture — the Whole Body

One page that stitches the individual concepts into a single picture: the
subsystems, their boundaries, and the inputs and outputs that connect them. Each
subsystem has a dedicated page; this is the map of how they wire together. Read
it after [[Overview]] to see the seams, then drill into the linked pages for the
detail.

The organizing thesis (see [[Song-Vocabulary]]): architecture is frozen music.
The **frozen half** is the nix layer — the score. The **performed half** is the
running desktop — the performance. **Drachma** is the one seam where they meet.

*Everything verifies green (flake check + the vm-boot check) — and the stack
**runs live** on yomi-strix.*

## Status — running live on yomi-strix

Aoide is a **built, switched, and logged-into walking skeleton**:
`nix flake check` is green, both packages build, the stack boots
headless in the `vm-boot` QEMU check — and yomi-strix runs it as its daily
graphical session. Live now: greetd → Hyprland → Quickshell (bar, dock,
gadgets, launcher, notifications, wallpaper — all one process, see
[[Quickshell]]), NetworkManager, zram, the Strix Halo amdgpu params, the aoide
binaries + git on PATH, flakes enabled; the previous [[dxflake]] generation
stays in the systemd-boot menu as the rollback. Every subsystem below is
marked on one of three rungs:

- **Implemented** — real code paths: the flake/walker/checks layer, the option
  contract, the drachma plumbing, the CLI trunk + MCP façade, the daemon and
  bridge skeletons, the three facets, song replay, the launcher, the gadget
  dock.
- **Stubbed** — the mutating CLI verbs (`rice gen/adopt/transpose`,
  `content *`, `make`, `update`, `onboard`) parse, audit, and exit 64 with a
  structured not-implemented payload; only the live action is deferred.
  (`rice lint` and `rice preview` are real — see [[Self-Ricing]].)
- **Future** — v1 drachma tiers, the functional rice loop (`gen`/`adopt`/
  `transpose`), the network-exposed Aoide connector.

The repo is deliberately **local-only** for now: no git remote, so [[Melete]]
fleet registration and its code-task flow wait until one exists. File-level
detail lives in [[Codebase]]; this page stays at the map altitude.

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
                  └──────────────►   DRACHMA   ◄──────────────┘
                                   the only seam
                        values frozen into the crystal,
                             sounded at runtime
```

Everything below is one of these two halves, or the drachma seam, or the agent
control plane that drives them.

## Master map — how the subsystems connect

Top is the driver (the agent); bottom is the pixels. Every arrow is a real input
or output, not an abstraction. Boxes marked *(stub)* exist as schema + audit
trail but exit 64 today.

```
                          ┌───────────────────────────┐
                          │           AGENT           │  claude CLI · melete · any shell
                          └─────────────┬─────────────┘
        commands / queries              │            ▲   events (via per-agent adapters)
                                        ▼            │
   ┌─────────────────────────────────────────────────────────────┐
   │  AGENT INTERFACE            aoide <cmd>   ·   aoide mcp serve │   [[Agent-Interface]]
   │  ONE schema (aoide schema --json) ──► CLI trunk + MCP façade  │   38 commands · exit 0/1/2/64
   └───────────────────────────────┬─────────────────────────────┘
                                    ▼
   ┌─────────────────────────────────────────────────────────────┐
   │  aoided  —  policy · single audit log (~/Aoide/log)          │   [[aoided]] · [[Governance]]
   │            default-deny event bus (per event class)           │
   │            REBUILD GATE  ◄───────────────  User admits        │
   └───┬───────────────────────┬────────────────────────┬────────┘
       ▼                       ▼                         ▼
  RICE ENGINE (stub)     CONTENT PIPELINE (stub)   NIX EVAL + REBUILD
  [[Self-Ricing]]        [[Content-Pipeline]]      [[Snowflake-Anatomy]]
   drachma·song/          discover→…→query          walker: modules/ +
   (lint/preview real)         │                    song/songbook/
       │                       ▼                         │
       ▼                  index (points in           resolves
  song/songbook/<song>     place, never copies)          │
  rice.nix + drachma.json                                  ▼
  songbook/ (write-back)                          ┌─────────────┐
       │                                          │   DRACHMA   │  aoide.drachma — one source
       └───────────────────────────────────────► └──────┬──────┘  [[drachma]]
                                    two fan-outs         │
                       ┌─────────────────────────────────┴───────────────┐
                       ▼ (rehearsal / live)                (recording / baked) ▼
        drachma emit {stage · hyprctl · osc}       rice.nix → facets + [[Stylix]]
          │              │            │                               │
          ▼              ▼            ▼                               ▼
  song/stage/drachma.json  hyprctl    terminal OSC        hyprland.conf · QML colors ·
          │              keywords   (color inject)      base16 for every nix app
          ▼
   Quickshell — DrachmaState.qml watches the stage file (hot-reload)
   [[Quickshell]]
      │  ▲
      │  └── reads song/stage/{sessions,hooks,graph}.json (roster + DAG surfaces)
      └──► widget click → shellbridge socket → hyprctl dispatch focuswindow
                              │
                    ┌─────────┴──────┐
                    │  shellbridge   │ ◄── Hyprland IPC socket ── [[Hyprland]]
                    │ [[shellbridge]] │
                    └────────────────┘
```

## Subsystem I/O — inputs, outputs, and status at a glance

Each row is one subsystem: what flows in, what flows out, and where it stands on
the implemented/stubbed ladder. This is the connective tissue the master map
draws.

| Subsystem            | Inputs                                         | Outputs                                             | Status                                     |
| -------------------- | ---------------------------------------------- | --------------------------------------------------- | ------------------------------------------ |
| [[Agent-Interface]]  | agent commands; `aoide schema --json`          | dispatched operations; structured `--json` results  | implemented (27 real verbs, 11 exit 64)    |
| [[aoided]]           | CLI+MCP operations; desktop events             | audit log (`~/Aoide/log`); default-deny event bus   | implemented (skeleton)                     |
| [[Self-Ricing]]      | prompt/wallpaper; `songbook/`; shipped default | `song/songbook/<song>/`; songbook append; preview   | stubbed (`rice lint`/`preview` real)        |
| [[Content-Pipeline]] | folders + manifests; Mneme API                 | in-place index; quarantine on lint fail             | stubbed (all verbs exit 64)                |
| [[drachma]]          | `aoide.drachma` (palette + component tiers)      | `song/stage/drachma.json`; baked facet + Stylix values | implemented (v0)                           |
| [[shellbridge]]      | unix-socket commands; Hyprland IPC             | atomic JSON in `song/stage/`; `hyprctl` dispatch    | implemented (accept loop live: `focuswindow`) |
| [[Quickshell]]       | `song/stage/*.json` (incl. drachma)            | widget socket commands; rendered surfaces           | implemented (9 real surfaces)              |
| [[Hyprland]]         | baked config + `hyprctl` keywords              | IPC event/state socket                              | implemented (greetd stubbed)               |
| [[Stylix]]           | base16 synthesized from the v0 palette         | themed config for every nix app                     | implemented (stands down on owned surfaces) |

## The drachma seam in detail — one source, two fan-outs, zero drift

Why preview and adopted state can never diverge: both derive from the same
`aoide.drachma` values. The drachma schema v0 (palette `bg/fg/accent/urgent` + component
tiers `bar`/`notif`/`window`, each field `null` → palette, with the fallback
applied **in the facets**) rides the external W3C design-tokens container
format; [[drachma]] (Node, wrapping Style Dictionary; bins `lint` /
`resolve` / `emit {stage,hyprctl,osc}`) is the engine. See [[drachma]].

```
                     aoide.drachma  (palette → component, v0)
                           │  single source of truth
             ┌─────────────┴──────────────┐
             ▼ REHEARSAL (live, gitignored) ▼ RECORDING (adopted, committed)
   drachma emit                     rice.nix ──► facets + Stylix
   ├─ stage: song/stage/drachma.json           │   (values baked at nix build)
   │         (atomic write; fully resolved)  ▼
   ├─ hyprctl: keyword dispatch         every nix-manageable target
   └─ osc: terminal color inject        GTK/Qt · terminal · editors
             │                          · browser · boot  (needs rebuild)
             ▼
   Quickshell hot-reload (DrachmaState.qml)
```

Rehearsal is the sketch (hot-reloads, no rebuild); recording is the truth
(durable, requires the gated rebuild). GTK/Qt surfaces need an app restart and
are adopt-only.

The baked side is carried by the three facets, all real:

- **quickshell** — declares nine surfaces with `owner = "quickshell"` (bar,
  notifications, launcher, osd, lockscreen, greeter, wallpaper, agentWidgets,
  sessionGraph); QML rsyncs from the store into the gitignored
  `~/Aoide/run/qml/` via home-manager activation (source stays
  `modules/facets/quickshell/qml/`; no `qml/` at the repo root);
  `DrachmaState.qml` watches the stage file for the live fan-out. Eight of
  the nine carry a live QML body: `AoideBar.qml` is the bar (its own popouts
  also carry the calendar and now-playing gadgets); `AoideLauncher.qml` is
  the keyboard-driven launcher (`SUPER+SPACE`); `AoideNotifications.qml` +
  `NotificationCard.qml` are the notification daemon (actions/inline-reply
  spike still pending); agentWidgets is the [[Gadget-Dock]] — `AoidePanel.qml`
  holding four gadgets (Conductor, Terminals, Meters, Power), a left-edge
  panel that peeks its fore-edge and opens fully on hot-edge hover or
  `SUPER+G`, all drachma-themed; osd, lockscreen, greeter, and wallpaper
  round out the set. `sessionGraph` remains declared but has no QML body —
  the DAG is rendered via `aoide graph view`/`aoide conductor`, not a desktop
  overlay ([[Session-Graph]]).
- **compositor** — [[Hyprland]]; the system layer holds session/portal wiring,
  the home-manager layer owns `hyprland.conf` with drachma baked at build and
  live-patched via `hyprctl` during rehearsal; greetd is stubbed.
- **stylix** — [[Stylix]]; a base16 scheme synthesized from the v0 palette,
  with colliding targets stood down on **both** the NixOS and home-manager
  layers for every Quickshell-owned surface.

The `surface-ownership` and `no-song-read` checks that police this seam are
real flake checks.

## The rice loop — where the agent writes

The self-ricing lifecycle overlays the map above: it produces drachma, previews
through the live fan-out, and only commits through the gate. See
[[Self-Ricing]]. Today `gen`, `adopt`, and `transpose` are exit-64 stubs;
`rice lint` (delegates to [[drachma]]) and `rice preview` (stages
`song/stage/drachma.json` for Quickshell hot-reload) are real.

```
  aoide rice gen <prompt|wallpaper>
        │   reads song/songbook/ (cross-cutting + the song's own design/) FIRST
        ▼
  rice lint   ──fail──►  reject + songbook note
        │ pass
        ▼
  rice preview   ──►  song/stage/drachma.json  ──►  Quickshell hot-reload
        │              (hyprctl/OSC dispatch not yet wired into preview)
        │                                         (REHEARSAL — nothing committed)
        ▼
  aoide rice adopt <name>   ◄─── User gates this step
        │
        ▼
  commit to song/songbook/<song>/   ──►  gated rebuild   ──►  RECORDING
        │                                     ↓ fleet-available: any host sets
        └──►  append songbook/ (learnings, preferences)      aoide.song = "<name>"
              ← the "self" in self-ricing
```

`song/` is the agent's **only** writable domain.

**Song replay is implemented.** `aoide.song` (nucleus option, default
`"default"`) selects the song a host performs; `lib/mkHost.nix` walks
`song/songbook/` exactly as it walks `modules/`, so a committed song
self-registers and self-gates on `config.aoide.song == "<name>"` — the same
discipline as a dendrite. The shipped standard is song `"default"` at
`song/songbook/default/` (the songbook's one upstream-owned, merge-only song). The song carries **drachma only**; the host is the
venue — its specifics and which instruments (facets, dendrites) are enabled.
Replay = same song, new venue (one line in `hosts/<host>/default.nix`);
transpose = new key, same venue. The `song-shape` check asserts every walked
songbook file is a `rice.nix`; song shape v0 is `CONTRACTS.md §5`. The
example is `song/songbook/sonata` (the selected light key): flip yomi-strix's
one line to another song and the whole drachma fan-out swaps (e.g. the shipped
`default` Mocha bg `#1e1e2e` → sonata peach-cream `#f4e9e2`). Full replay treatment:
[[Song-Vocabulary#Replay — any song, any host]].

Inherited structure (nucleus, facets) changes by upstream merge only; new
dendrite branches are additive. Mutation policy is encoded as radial distance
from the nucleus — see [[Snowflake-Anatomy]] and [[Governance]].

## The content pipeline — how knowledge enters

Runs beside the rice loop off the same daemon. The approve gate is the
structural break against injection. See [[Content-Pipeline]]. All five verbs
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

## The control plane — one gate, one log, two doors

Everything an agent can do is one command schema with two front doors that
cannot drift, funnelled through a single policy/audit surface. See
[[Agent-Interface]] and [[Governance]].

```
   agent ──► CLI trunk ────┐
                           ├──►  aoided  ──►  policy · GATE · audit(~/Aoide/log)
   agent ──► MCP façade ───┘         (generated from the same schema)
```

This is shipped code: the Rust crate ([[aoide-cli]]) installs two binaries,
`aoide` and `aoided`. `aoide schema --json` is the machine-readable source of
truth; the stdio MCP façade (`aoide mcp serve --stdio`) generates its tool list
from it, one-to-one. The tree holds **38 commands** — real (27): `guide`,
`schema`, `rice lint`, `rice preview`, `rice mint`, `cover set`, `mcp serve`,
`daemon`, `shellbridge`, `conduct`, `conductor`, `adapter melete`, and the
15-verb `graph` group (the
[[Session-Graph]] DAG viewer + management layer over projects and sessions,
incl. `graph send`/`wrap`/`reap`, all real); stubs (11, exit 64):
`rice gen/adopt/transpose`, the 5-verb `content` group, `make`, `update`,
`onboard`. Exit codes are contractual: 0 ok, 1 error, 2 usage, 64
not-implemented.

On the host, the plane runs as systemd user units, all from the nucleus:
`aoided`, `shellbridge` (socket `$XDG_RUNTIME_DIR/aoide/shellbridge.sock`),
`aoide-melete-adapter`, and `aoide-mcp` (gated on `aoide.mcp.enable`, default
false) — plus the `aoide-obsidian-register` oneshot from the shipped dendrite.
Live state lands in `song/stage/{drachma,sessions,hooks,projects,graph}.json`
(the last two from the [[Session-Graph]] layer). `lib/mkHost.nix`
injects `pkgs.aoide` / `pkgs.drachma` by overlay from the **same**
`callPackage` paths as the flake's `packages` output, so the units and the
flake build one binary, not two.

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
- One audit surface: both doors append to the single audit log file
  (`~/Aoide/log`, the `aoide.auditLog` option) — there is no per-door log.
- Trust boundary: forwarded notification text is untrusted data. The melete
  adapter forwards metadata only; an app title must never reach the agent as a
  command — a hard architectural constraint.

## Where each subsystem lives in the tree

The frozen half maps onto the repo layout as built (see [[Snowflake-Anatomy]]
for the layer anatomy, [[Codebase]] for file-level detail):

```
~/Aoide/
├── flake.nix        inputs: nixpkgs · home-manager · stylix · quickshell · hyprland
│                    outputs: nixosConfigurations.yomi-strix · packages.{aoide,drachma}
│                    · checks · devShells · formatter
├── lib/             walk.nix (dendritic walker) · mkHost.nix (host assembly + pkgs
│                    overlay) · checks.nix (surface-ownership · no-song-read · song-shape)
│                    · vmTest.nix (the vm-boot headless QEMU check)
├── modules/         the snowflake — walker-discovered layers
│   ├── nucleus/     options.nix (THE contract) · aoided · shellbridge · melete-adapter
│   │                · packages.nix (aoide + drachma + git on PATH) · nix.nix (flakes on)
│   ├── dendrites/   20 opt-in features (bash, nh, git, kitty, neovim, starship,
│   │                mcfly, btop, yazi, fastfetch, devtools, cli, fonts, hyprland,
│   │                obsidian, melete, mneme, firefox, screenshot, vision) ← additive
│   └── facets/      quickshell · compositor · stylix          ← render surfaces (drachma-only)
├── hosts/           common/ + yomi-strix/ (flags + the aoide.song selector; a real
│                    hardware profile, switched live and running as the daily desktop)
├── pkgs/            aoide/ (Rust: aoide + aoided) · drachma/ (Node: drachma)
├── song/            songbook/{default,sonata}/ — rice.nix · drachma.json ·
│                    palette/ · sounds/ · icons/ · widgets/ · design/ (per song);
│                    covers/ — shared wallpaper library, referenced by rice.nix;
│                    stage/ + auditions/ runtime (gitignored)
├── docs/BUILD.md    module-authoring conventions
├── CONTRACTS.md     versioned contracts (drachma · dendrite · schema · stage · song shape)
├── AGENTS.md        tier-0 agent guide (aoide guide prints the same map)
└── log              single audit log — runtime, gitignored
```

Runtime dirs (`song/{stage,auditions}`, root `log`, `index/`,
`catalog/`) are gitignored and created at runtime; `song/songbook/**` is
versioned score, legitimately walked at eval.

`hosts/` knows dendrites; dendrites never know hosts. Facets read only
`aoide.drachma` (and declare `aoide.surfaces`); no module reads another module.
The coupling discipline is contractual — the flake's checks (`surface-ownership`,
`no-song-read`, `song-shape`, plus building both packages, plus the `vm-boot`
headless boot of the assembled stack) fail eval on violation.

## Related

- [[Overview]]
- [[Codebase]]
- [[Snowflake-Anatomy]]
- [[Desktop-Architecture]]
- [[drachma]]
- [[Self-Ricing]]
- [[Content-Pipeline]]
- [[Agent-Interface]]
- [[Governance]]
- [[Song-Vocabulary]]
