---
type: concept
created: 2026-07-26
updated: 2026-08-14
tags: [aoide, architecture, desktop, livery, pipeline]
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
running desktop — the performance. **Livery** is the one seam where they meet.

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
  contract, the livery plumbing, the CLI trunk + MCP façade, the daemon and
  bridge skeletons, the three facets, song replay, the launcher, the gadget
  dock.
- **Stubbed** — the mutating CLI verbs (`rice declare/transpose`,
  `content *`, `make`, `update`, `onboard`) parse, audit, and exit 64 with a
  structured not-implemented payload; only the live action is deferred.
  (`rice lint`/`rice stage`/`rice compose`/the `rice draft` group are real —
  see [[Self-Ricing]]. There is no `rice gen`: cut outright, not stubbed —
  see [[aoide-cli]].)
- **Future** — v1 livery tiers, `rice declare`/`rice transpose` (still
  stubs), the network-exposed Aoide connector.

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
                  └──────────────►   LIVERY    ◄──────────────┘
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
   livery·song/          discover→…→query          walker: modules/ +
   (lint/preview real)         │                    song/songbook/
       │                       ▼                         │
       ▼                  index (points in           resolves
  song/songbook/<song>     place, never copies)          │
  rice.nix + livery.json                                   ▼
  songbook/ (write-back)                          ┌─────────────┐
       │                                          │   LIVERY    │  aoide.livery — one source
       └───────────────────────────────────────► └──────┬──────┘  [[livery]]
                                    two fan-outs         │
                       ┌─────────────────────────────────┴───────────────┐
                       ▼ (rehearsal / live)                (recording / baked) ▼
        livery emit {stage · hyprctl · osc}        rice.nix → facets + [[Stylix]]
          │              │            │                               │
          ▼              ▼            ▼                               ▼
  song/stage/livery.json   hyprctl    terminal OSC        hyprland.conf · QML colors ·
          │              keywords   (color inject)      base16 for every nix app
          ▼
   Quickshell — LiveryState.qml watches the stage file (hot-reload)
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
| [[Self-Ricing]]      | prompt/wallpaper; `songbook/`; shipped standard | `song/songbook/<song>/`; songbook append; preview   | stubbed (`rice lint`/`preview` real)        |
| [[Content-Pipeline]] | folders + manifests; Mneme API                 | in-place index; quarantine on lint fail             | stubbed (all verbs exit 64)                |
| [[livery]]           | `aoide.livery` (palette + component tiers)     | `song/stage/livery.json`; baked facets + Stylix     | implemented (v0)                           |
| [[shellbridge]]      | unix-socket commands; Hyprland IPC             | atomic JSON in `song/stage/`; `hyprctl` dispatch    | implemented (accept loop live: `focuswindow`) |
| [[Quickshell]]       | `song/stage/*.json` (incl. livery)             | widget socket commands; rendered surfaces            | implemented (9 real surfaces)              |
| [[Hyprland]]         | baked config + `hyprctl` keywords              | IPC event/state socket                              | implemented (greetd stubbed)               |
| [[Stylix]]           | base16 synthesized from the v0 palette         | themed config for every nix app                     | implemented (stands down on owned surfaces) |

## The livery seam in detail — one source, two fan-outs, zero drift

Why preview and adopted state can never diverge: both derive from the same
`aoide.livery` values. The livery schema v0 (palette `bg/fg/accent/urgent` + component
tiers `bar`/`notif`/`window`, each field `null` → palette, with the fallback
applied **in the facets**) rides the external W3C design-tokens container
format; [[livery]] (native Rust in `crates/song/src/livery/`; verbs `lint` /
`resolve` / `emit {stage,hyprctl,osc,file}`) is the engine. See [[livery]].

```
                     aoide.livery  (palette → component, v0)
                           │  single source of truth
             ┌─────────────┴──────────────┐
             ▼ REHEARSAL (live, gitignored) ▼ RECORDING (adopted, committed)
   livery emit                      rice.nix ──► facets + Stylix
   ├─ stage: song/stage/livery.json            │   (values baked at nix build)
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
  sessionGraph); QML rsyncs from the store into the gitignored
  `~/Aoide/run/qml/` via home-manager activation (source stays
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
  the DAG is rendered via `aoide graph view`/`aoide conductor`, not a desktop
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
  aoide rice compose <name> [--from <song>]
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
  aoide rice declare <name>   ◄─── User gates this step
        │
        ▼
  commit to song/songbook/<song>/   ──►  gated rebuild   ──►  RECORDING
        │                                     ↓ fleet-available: any host sets
        └──►  append songbook/ (learnings, preferences)      aoide.song = "<name>"
              ← the "self" in self-ricing
```

`song/` is the agent's **only** writable domain.

**Song replay is implemented.** `aoide.song` (nucleus option, default
`"sonata"`) selects the song a host performs; `lib/mkHost.nix` walks
`song/songbook/` exactly as it walks `modules/`, so a committed song
self-registers and self-gates on `config.aoide.song == "<name>"` — the same
discipline as a dendrite. The shipped standard is song `"sonata"` at
`song/songbook/sonata/` — upstream-owned and evolving, exactly like any
other upstream-owned tree (nucleus, facets): upstream MAY still update or
iterate on it. Every OTHER song — anything composed via `rice compose`
under a different name — is clone-owned; upstream never touches it, an
absolute guarantee unchanged by `sonata` being both shipped and actively
iterated. The song carries **livery only**; the host is the
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

## The control plane — one gate, one log, three doors

Everything an agent can do is one command schema with three front doors that
cannot drift, funnelled through a single policy/audit surface. See
[[Agent-Interface]], [[A2A-Door]], and [[Governance]].

```
   agent ──► CLI trunk ─────┐
   agent ──► MCP façade ─────┼──►  aoided  ──►  policy · GATE · audit(~/Aoide/log)
   agent ──► A2A door ───────┘         (all generated from the same schema)
```

This is shipped code: the Rust crate ([[aoide-cli]]) installs two binaries,
`aoide` and `aoided`. `aoide schema --json` is the machine-readable source of
truth; the stdio MCP façade (`aoide mcp serve --stdio`) generates its tool list
from it, and the [[A2A-Door]]'s AgentCard is derived from the same schema — all
one-to-one. The tree holds **60 commands** — real (50): `guide`,
`schema`, `rice lint`, `rice stage`, `rice compose`, the 3-verb `rice draft`
group (`save`/`list`/`drop`), the 4-verb `rice mode`
group (`status`/`stage`/`declarative`/`draft`), `cover set`, `mcp serve`,
`daemon`, `shellbridge`, `conduct`, `conductor`, `adapter melete`, the 3-verb
`livery` group (`lint`/`resolve`/`emit` — the native note engine), the 5-verb
`a2a` door group (`a2a serve` + `a2a agent add/list/remove/send`), the
5-verb `peer` group (`peer add/list/remove/pull/status` — cross-device peer
federation, [[Peer-Federation]]), `usage`,
`hooks install`, the `shell reload` group (the Quickshell IPC hot-reload
trigger — rebuilds the whole scene from `shell.qml` in-process, picking up
dynamically-loaded widget/facet QML the file watcher can't track), and the
15-verb `graph` group (the
[[Session-Graph]] DAG viewer + management layer over projects and sessions,
incl. `graph send`/`wrap`/`reap`, all real); stubs (10, exit 64):
`rice declare/transpose`, the 5-verb `content` group, `make`, `update`,
`onboard`. There is no `rice gen` — cut outright (khoa 2026-08-14), not left
as a stub. Exit codes are contractual: 0 ok, 1 error, 2 usage, 64
not-implemented.

On the host, the plane runs as systemd user units, all from the nucleus:
`aoided`, `shellbridge` (socket `$XDG_RUNTIME_DIR/aoide/shellbridge.sock`),
`aoide-melete-adapter`, and `aoide-mcp` (gated on `aoide.mcp.enable`, default
false) — plus the opt-in `aoide-a2a` ([[A2A-Door]], gated on `aoide.a2a.enable`)
and `aoide-usage` (gated on `aoide.usage.enable`) units, and the
`aoide-obsidian-register` oneshot from the shipped dendrite.
Live state lands in `song/stage/{livery,sessions,hooks,projects,graph}.json`
(the last two from the [[Session-Graph]] layer). `lib/mkHost.nix`
injects `pkgs.aoide` by overlay from the **same**
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
- One audit surface: every door (CLI, MCP, A2A) appends to the single audit log
  file (`~/Aoide/log`, the `aoide.auditLog` option) — there is no per-door log.
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
│                    · vmTest.nix (the vm-boot headless QEMU check)
├── modules/         the snowflake — walker-discovered layers
│   ├── nucleus/     options.nix (THE contract) · aoided · shellbridge · melete-adapter
│   │                · packages.nix (aoide + git on PATH) · nix.nix (flakes on)
│   ├── dendrites/   20 opt-in features (bash, nh, git, kitty, neovim, starship,
│   │                mcfly, btop, yazi, fastfetch, devtools, cli, fonts, hyprland,
│   │                obsidian, melete, mneme, firefox, screenshot, vision) ← additive
│   └── facets/      quickshell · compositor · stylix          ← render surfaces (livery-only)
├── hosts/           common/ + yomi-strix/ (flags + the aoide.song selector; a real
│                    hardware profile, switched live and running as the daily desktop)
├── pkgs/            aoide/ (Rust: aoide + aoided; one crate today — a pi-
│                    style single-charter crate split is a target blueprint,
│                    not yet built, see [[Package-Layout]])
├── song/            songbook/sonata/ (shipped standard) — rice.nix · livery.json ·
│                    palette/ · sounds/ · icons/ · widgets/ · design/ (per song);
│                    covers/ — shared wallpaper library, referenced by rice.nix;
│                    stage/ + auditions/ runtime (gitignored)
├── docs/BUILD.md    module-authoring conventions
├── CONTRACTS.md     versioned contracts (livery · dendrite · schema · stage · song shape)
├── AGENTS.md        tier-0 agent guide (aoide guide prints the same map)
└── log              single audit log — runtime, gitignored
```

Runtime dirs (`song/{stage,auditions}`, root `log`, `index/`,
`catalog/`) are gitignored and created at runtime; `song/songbook/**` is
versioned score, legitimately walked at eval.

`hosts/` knows dendrites; dendrites never know hosts. Facets read only
`aoide.livery` (and declare `aoide.surfaces`); no module reads another module.
The coupling discipline is contractual — the flake's checks (`surface-ownership`,
`no-song-read`, `song-shape`, plus building both packages, plus the `vm-boot`
headless boot of the assembled stack) fail eval on violation.

## Related

- [[Overview]]
- [[Codebase]]
- [[Package-Layout]]
- [[Snowflake-Anatomy]]
- [[Desktop-Architecture]]
- [[livery]]
- [[Self-Ricing]]
- [[Content-Pipeline]]
- [[Agent-Interface]]
- [[Governance]]
- [[Song-Vocabulary]]
