---
type: concept
created: 2026-07-26
updated: 2026-08-30
tags: [aoide, architecture, nix, flake, rust, node]
---

# Codebase — How the Built Repo Works

The implementation of Aoide at `~/Aoide` (the dev git checkout, reached via
`$AOIDE_FLAKE_ROOT`; runtime state hangs off `$AOIDE_ROOT`, default
`~/.aoide`): the
flake, the catalogue and the composition rules, the option contract, the
systemd/service map, the runtime
contracts, and what is real versus stubbed. It is the map from the design
concepts ([[Snowflake-Anatomy]], [[Full-Architecture]]) to the files on disk.

## The flake

`flake.nix` is the fixed skeleton — later waves ADD files in their own dirs and
never touch it. Inputs: `nixpkgs` (unstable), `home-manager`, `stylix`,
`quickshell`, `hyprland` (the last four all `follows` nixpkgs where they can).
Outputs, all tolerant of empty layers so eval stays robust:

- `nixosConfigurations.yomi-strix` — assembled by the constructor,
  `lib/aoideos.nix` over habit's composition (`inputs.habit`), from the host's own record.
- `packages` — **auto-discovered** by `lib/pkgs.nix` from `pkgs/<name>/default.nix`
  (`callPackage`, `_`-shelving); currently `{aoide, chatgpt-linux, eidolon,
  hyprglass, iconify-data, kimi-code, lyra-shell, lyra-songbook, melete,
  mneme}` plus `default` (= aoide), `aoide` being a self-flaked path input.
  Adding a package is one folder —
  this file never changes.
- `nixosModules` / `overlays.default` / `lib` / `songbookRoot` / `aoideOptions`
  — the export surface a consuming flake builds against: one
  `nixosModules.<name>` per catalogue entry plus `nucleus` (the module that
  closes over Aoide's own inputs), the base package overlay, the constructor
  files (`lib.composition`, `lib.livery`, `lib.songbook`, `lib.catalogue`),
  Aoide's own songbook directory for a host that performs Aoide's songs, and the
  derived `aoide.*` option list `lyra onboard` reads.
- `checks` — the contractual coupling discipline from `lib/checks.nix`
  (`surface-ownership`, `song-runtime-untracked`, `song-shape`, `fmt`,
  `discovery`, `phantom-commands`, `nix-independence`, `portability`,
  `nix-lint`, `livery-fanout`, `generator-offline`, `generator-relocatable`),
  one auto-generated `pkg-<name>` per discovered package, plus the `vm-boot`
  headless boot test (below).
- `devShells.default` — Rust (cargo/rustc/clippy/rust-analyzer) + nix
  tooling (nixfmt/nil/deadnix/statix).
- `formatter` — nixfmt.

## The catalogue, the constructor (`lib/`)

**`modules/default.nix` is the catalogue**, and it is plain data, not a module:
one `name = path;` line per dendrite, read before any module graph exists. That
line is both what makes a capability selectable and what puts it in the full
tree, and it is the ONE place a dendrite file is named. A `_`-prefixed file is
not catalogued, which is the **shelving opt-out** (`_example.nix`, `_scratch/`)
— hidden without being deleted. `aggregations` and `overrides` are the sibling
records, found one level deep by their own directories' `default.nix`. This is
[[dxflake]]'s pattern, implemented in-house, and
the concrete case of [[Plugin-Architecture]]'s discovery-by-existing rule.

**habit's composition** (`inputs.habit`, re-exported as `lib.composition`) is the selection engine: the host module's `habit.*` keys are evaluated
in an ordinary `evalModules` pass that knows nothing about NixOS, and the
platform module list is assembled from the result — only what selection kept is
imported. It resolves the aggregations a host (or one of its users) took, the
provider a registry entry names, and the override records that match; the
aggregate whole-tree import and a selected lane are refused together by name,
because both would import the same `body`.

**`lib/pkgs.nix`** is the same discovery idea for `pkgs/`: it reads `../pkgs`, keeps each
directory without a `_` prefix that holds a `default.nix`, and maps it to
`callPackage`. One source feeds the flake `packages` output, the host + vm
overlays, and the `pkg-<name>` checks — so a package self-registers everywhere
from a single new folder. Its `overlay` form guards each name against
accidentally masking a nixpkgs attribute; a deliberate shadow is listed in
`intentionalShadows`, and the walker yields a name it supplies only to the
overlays listed in `intentionalOverrides` (the lyra lane replacing
`lyra-songbook` with a host's built-in songs is the one case today); a
non-standard build arg goes through a documented
`//` escape hatch (kept in this file to preserve the single source).

**`lib/aoideos.nix`** is the host constructor: `hostNames` is discovered from
`hosts/` (every immediate child with a `default.nix`, minus the `_`-prefixed
templates), and `mkHost name` assembles that host's outputs — its
`nixosSystem`, its `inventory`, and the module values a consumer may want.
Nothing in it names a machine or imports a dendrite file. It closes over
Aoide's own flake inputs once, in `nucleusModule` (the same value exported as
`nixosModules.nucleus` and passed as `_module.args.aoideInputs`), appends the
**discovered-packages overlay** from `lib/pkgs.nix` — the *same* source the
flake's `packages` output and the `pkg-<name>` checks read, so paint lanes
reference `pkgs.aoide` / `pkgs.lyra-shell` … without drift — and wires the two
song hooks (`selectionModules` puts `habit.song.declared`/`habit.song.available` on the host
module; `extraModulesFor` turns the selection into the built-in songs'
`rice.nix` files and the `aoide.song`/`aoide.songbook.builtIn` facts). Each
song's `rice.nix` guards itself with
`lib.mkIf (config.aoide.song == "<name>")`, so a host performs the one song it
declares and builds in the ones it lists (see [[Song-Vocabulary#Replay — any song, any host]]).

**`lib/songbook.nix`** is the songbook generator and its discovery (the one
generator two callers share: the lyra lane's build-time `manifest.json`/
`registry.json` derivation, and the `songbookManifest` flake output
[[Self-Ricing|`rice stage`]] shells out to via `nix eval --json` for its
hot-sync half). The same generator also feeds `pkgs/lyra-songbook`, which
bakes the built-in songs plus a prebaked
`manifest.json`/`registry.json`/`builtin.json` into `share/lyra/songbook` — the shipped
templates a repo-less host's registry/manifest regen reads nix-free
(`CONTRACTS.md` §4). **`lib/livery.nix`** resolves `aoide.livery.override.*`, the
host-set venue-recolour tier (`CONTRACTS.md` §1). **`lib/song.nix`** builds
each song's widget records from its `_widgets/<slot>.nix` functions — the nix
analogue of `WidgetSlot.qml` — and `lib/options.nix` is the `aoideOptions`
walk `lyra onboard` reads.

**`lib/checks.nix`** rides as flake `checks` — the contractual coupling
discipline, written to throw legibly at eval time on a violation:

- **`surface-ownership`** — every declared `aoide.surfaces.<name>` names a
  non-empty owner (a paint lane asserting it is the sole owner of a render surface).
- **`song-runtime-untracked`** — no `song/` runtime dir (`stage/`,
  `auditions/`, `declared/`) exists in the source tree at all, so the nix build
  can never come to depend on ephemeral runtime state. `stage/` stays non-load-
  bearing structurally. This boundary is about runtime dirs only; committed
  `song/songbook/**` is versioned score and is legitimately read at eval.
- **`song-shape`** — three assertions on the committed songbook: no stray `.nix`
  outside a song's `rice.nix`/`_widgets/`, no `../` path literal in a song's
  `.nix` text, and every song carrying both `rice.nix` and `livery.json`
  (the host-agnostic song discipline, `CONTRACTS.md §5`).

Alongside these, `flake.nix` auto-generates a `pkg-<name>` check per discovered
package (from `lib/pkgs.nix`) so every package builds under `nix flake check`.

**`tests/vm-boot.nix`** wires `checks.<system>.vm-boot` — a
headless QEMU boot of the whole stack via `pkgs.testers.runNixOSTest`
(4 GiB / 4 vCPU, KVM). Its node is assembled from the **same** parts
the constructor uses — habit's composition `mkNixosModules` over an inline
host module, the nucleus lane, the
home-manager modules, the pkgs overlay, and mirrored `specialArgs`
(`host = "vm-test"`, inputs, username, system; `node.pkgsReadOnly = false`
so the overlay applies) — so the test boots the real assembly, not a
replica. It asserts: `multi-user.target` reached; `aoide` on
PATH with `schema --json` reporting a non-zero command count (the golden
snapshot in `crates/cli/src/registry.rs` owns the exact figure; the boot
test only proves the surface is there) and `guide` exiting
0; greetd enabled (a Hyprland respawn loop on the virtual GPU is tolerated);
linger active with the `aoided` user unit finishing
`Result=success` (the skeleton binaries seed state and exit 0); stage files
seeded at `schemaVersion` 0; and a `project add → graph` round-trip landing
`graph.json` (every stage mutation restages it automatically, so there is no
separate emit step to round-trip through). The test node trims the stylix,
quickshell and lyra lanes and names no song
(headless closure cost; the compositor stays for greetd). The script runs in
about 16 s of wall clock: `nix build .#checks.x86_64-linux.vm-boot -L`. It
also confirmed in-VM that the unit's `AOIDE_STAGE_DIR` and the binary's
fallback agree on the same stage path.

**Test scope.** The test node's `systemPackages` carries only `jq`; the real
install path (`nucleus/packages.nix`) is what puts `aoide` on
the box, so the vm-boot PATH assertion (above) verifies the nucleus install
rather than the test's own scaffolding. **eval green + build green + VM
green ≠ complete** — a VM test proves only what it does not provide for
itself.

## The option contract (`modules/nucleus/options.nix`)

The versioned seam every other module builds against (livery schema v0,
`CONTRACTS.md §1`). It declares options and eval-clean defaults only — it wires
no behaviour, so an empty config evaluates. The surface:

- `aoide.enable` (master switch), `aoide.user` (default `"khoa"`, owner of the
  `~/Aoide` clone).
- `aoide.root` (default `~/.aoide`, exported as `AOIDE_ROOT`) — the runtime
  root: `song/stage/`, `song/declared/`, `state/` (+ `state/stage/`),
  `run/qml/`, the composed
  host `song/songbook/`, the audit log. `aoide.checkout` (default `~/Aoide`,
  exported as `AOIDE_FLAKE_ROOT`) — the dev git checkout: `soundcheck`'s scan
  root, `rice declare`'s commit-in target, the songbook `nix eval` registry
  regen.
- `aoide.song` (`nullOr str`, default `null`) — which song this host performs.
  Null performs no song: the paint lanes read the null and deploy nothing (no
  QML tree, no shell service). Set once in `hosts/<host>/default.nix` (and
  `habit.song.available` with it, for what the host keeps built in); each
  song's `rice.nix` guards itself
  with `lib.mkIf (config.aoide.song == "<name>")`. See [[Song-Vocabulary#Replay — any song, any host]].
- `aoide.livery` — the v0 livery schema: closed `palette.{bg,fg,accent,urgent}`
  (base16, permissive hex type) + optional component tiers `bar.*` / `notif.*` /
  `window.*` (each `nullOr` hex, `null` → palette), the additive-optional tiers
  (`base16`, `geometry.*`, `fonts.monospace`, `wallpaper`), and an `override.*`
  venue tier (host-set only, [[livery|resolved by `lib/livery.nix`]]).
- `aoide.arrangement` — the v1 arrangement schema: which widget/surface types
  a song brings into existence. `aoide.livery`, `aoide.arrangement` and
  `aoide.surfaces` are the whole paint-lane read set, with the core scalars and
  the song selection root `AGENTS.md` house rule 5 enumerates — no module reads
  another module's options.
- `aoide.surfaces.<name>.owner` — the surface-ownership registry.
- `aoide.mcp.enable`, `aoide.a2a.enable`, `aoide.usage.enable`,
  `aoide.secrets.enable` — each off by default (house policy); `aoide.auditLog`
  (default `$AOIDE_ROOT/log`, i.e. `~/.aoide/log`).

## The host profile — yomi-strix is real

`hosts/yomi-strix/` is a **bootable first-iteration profile**, ported from
[[dxflake]] (the template)
and trimmed to essentials:

- **UUID-pinned `fileSystems`** matching the layout disko created on the box
  (1 G vfat ESP + ext4 root on the single NVMe). Aoide does **not** manage
  disks, so the live layout is pinned by UUID.
- systemd-boot UEFI; the Strix Halo scan-module set with early-KMS `amdgpu`;
  `linuxPackages_latest` with `amdgpu.gttsize=24576` /
  `ttm.pages_limit` kernel params (the RDNA 3.5 iGPU maps 24 GiB of the
  unified 32 GB pool); amdgpu userspace graphics for the Hyprland lane;
  NetworkManager; zramSwap.
- Not present on this host: disko, inference/ROCm, the Melete role,
  bluetooth, wireguard.

**Runs live.** A [[Rebuild-Gate]] switch runs as a detached `systemd-run`
unit (`nix-env --profile` set + `switch-to-configuration switch`) and
activates in about 8 s. The prior dxflake generation stays in the
systemd-boot menu, so rollback is one boot-menu pick away. Live: greetd
active, the Hyprland session entry present, NetworkManager, zram, and the
Strix Halo amdgpu params all in effect; `aoide` + git on PATH
and flakes enabled. The graphical session (greetd → Hyprland → Quickshell)
runs as the daily desktop — bar, dock, gadgets, launcher, notifications, and
wallpaper all live in one process.

## The systemd user-unit map (nucleus + a dendrite)

All nucleus services are user services gated on `aoide.enable`, keyed into
`graphical-session.target`:

| Unit | From | Notes |
|---|---|---|
| `aoided` | `nucleus/aoided.nix` | runs `${pkgs.aoide}/bin/aoided`; env `AOIDE_AUDIT_LOG`, `AOIDE_USER` |
| `shellbridge` | `dendrites/lyra/shellbridge.nix` | runs `lyra shellbridge --run`; `RuntimeDirectory=aoide` for the socket |
| `aoide-graph-reap` | `dendrites/lyra/shellbridge.nix` | timer, ~12s interval; the liveness reaper sweeping sessions a killed terminal could never mark `done` |
| `aoide-melete-adapter` | `nucleus/melete-adapter.nix` | runs `aoide adapter melete --run`; `AOIDE_ADAPTER_SUBSCRIBE` allow-list |
| `aoide-mcp` | `nucleus/aoided.nix` | **gated on `aoide.mcp.enable`**; `bindsTo` aoided |
| `aoide-a2a` | `nucleus/aoided.nix` | **gated on `aoide.a2a.enable`**; the [[A2A-Door]] serve unit |
| `aoide-usage` + timer | `nucleus/aoided.nix` | **gated on `aoide.usage.enable`**; runs `aoide usage` on `aoide.usage.interval` |
| `aoide-secrets-serve` | `nucleus/secrets.nix` | **gated on `aoide.secrets.enable`**; SYSTEM (not user) service, own uid `aoide-secrets`, anchored to `multi-user.target` — see [[Secrets-Broker]] |
| `aoide-obsidian-register` | `dendrites/obsidian.nix` | oneshot; registers a window class with shellbridge |

`systemd.user.tmpfiles.rules` create `${config.aoide.root}/log` (0700),
`${config.aoide.root}/song/stage` (0755, rice staging),
`${config.aoide.root}/state` (0700), and `${config.aoide.root}/state/stage`
(0755, conducting state) at runtime — with the default root, `~/.aoide/log`,
`~/.aoide/song/stage`, `~/.aoide/state`, `~/.aoide/state/stage`;
systemd-tmpfiles deduplicates the shared rules declared in both
aoided and shellbridge.

Two **non-service nucleus modules** close baseline gaps, both gated on
`aoide.enable`:

- **`nucleus/packages.nix`** puts `pkgs.aoide` on the
  **system profile**. The units never needed this (their `ExecStart` lines are
  absolute store paths), but keybinds and interactive sessions invoke by
  name. It also installs **git**, which is load-bearing rather than dev
  comfort: nix flake operations on the user's own clone require it.
- **`nucleus/nix.nix`** enables the `nix-command` + `flakes` experimental
  features, required for the flake-native system to evaluate itself.

## Runtime contracts (socket + stage files)

Live-side state, all under the runtime root (`$AOIDE_ROOT`, default
`~/.aoide`), none committed, none load-bearing for the build:

- **Socket:** `$XDG_RUNTIME_DIR/aoide/shellbridge.sock` — the one outbound
  channel from QML; adapters and widgets bind exactly this path, never compute
  it.
- **Stage files** split by owner into two trees (command-defrag lane S1):
  `song/stage/` holds rice/paint staging — `livery.json` (resolved livery
  colours, written by [[livery]]'s `rice stage`/`cover set`/other emitters at
  rehearsal — both refuse under `rice mode declarative`, see
  [[Self-Ricing#Staging vs Declarative Mode]] — and reseeded from the active
  song's committed
  `song/songbook/<song>/livery.json` on every activation by
  `home.activation.aoideSeedStage` in `modules/dendrites/lyra/default.nix`
  — a write-temp-then-rename script that injects the same `"song"` field
  `rice stage` writes, so a host that boots without ever staging still
  carries a correct live stage twin from the baked default; when `mode.json`
  names a staged or drafted song, `lyra reload`, run by the lane's
  `aoide-rice-reload` unit at every login and after every switch, re-stages it
  over that seed), `mode.json`
  (the staging/declarative mode marker — absent reads as `declarative`), and
  `grimoire.json` (QML-only writer). That same seed also publishes the
  DECLARED twin at `song/declared/livery.json` — the declared song's
  committed notes with the venue recolour applied, one `jq` run to two
  destinations (`CONTRACTS.md §4`). It exists because the live stage is
  rewritten by the runtime writers and so cannot itself state what the venue
  declared: `rice mode declarative` re-pins from it, and its `"song"` field
  names the declared song. The staging writers derive every song from the
  runtime songbook and, for the declared one, lay the seed's
  `song/declared/venue.json` (the slots the venue recolours and the geometry the host
  sets, `{}` with neither) over it. `state/stage/` holds
  CONDUCTING
  state — `sessions.json` (agent session roster, written by
  [[shellbridge]]; records may carry an additive optional `parentSessionId`),
  `hooks.json` (live Claude Code hook phases), `projects.json` (the project
  registry, kept by `aoide project add/remove/list`), `graph.json` (the
  resolved project/session DAG, restaged automatically by every graph
  mutation for Quickshell — see [[Session-Graph]]), `herald.json` (the
  notification ledger the Quickshell herald draws from — the shellbridge
  daemon is the single writer), and `pending.json` (the held-injection queue
  `send`/the A2A door write when their gate doesn't clear immediate
  delivery, resolved by `session pending list/approve/deny`). `cover.json`
  (the wallpaper note, written by `cover set`) is rice staging and stays
  under `song/stage/`. Each has a v0 shape in `CONTRACTS.md §4`; writes are
  atomic (write-temp-then-rename), and the graph rewriters round-trip unknown
  fields so concurrent writers never lose data.
- **Stage-dir resolution** (`CONTRACTS.md §4`): two functions, one per tree —
  `stage_dir()` for `song/stage/` and `conducting_stage_dir()` for
  `state/stage/`. Both resolve `$AOIDE_STAGE_DIR` when set to an absolute
  path (the unit sets it; empty/relative ignored) first, so an override
  relocates both trees as a unit; with no override each falls back to its
  own tree under the runtime root (`$AOIDE_ROOT` absolute-path-wins, else
  `~/.aoide`; `$AOIDE_STATE_DIR` relocates `state/` alone with the same
  precedence) — the documented CLI ↔ unit seam,
  pinned by a serialized precedence test. On first run the binaries run a
  one-shot root migration (`fs::migrate_root_once()`) moving pre-existing
  `~/Aoide/{song/stage,state,log}` into the root, each piece gated on its
  own override being unset. The first no-override resolution
  of `conducting_stage_dir()` runs a one-shot migration moving any of the
  six conducting files still sitting at the old `song/stage/` location into
  `state/stage/`, never clobbering a fresher file already there and never
  touching a rice file.

## Repo-file roles

- **`CONTRACTS.md`** — §0 design philosophy plus the versioned contracts §1–8:
  note schema, dendrite shape, `aoide schema --json` output, stage-file
  formats, song shape, A2A door, node federation, screen capture + pointer
  synthesis. The `checks` fail a merge that breaks one; bumping a version
  needs a playbook migration.
- **`AGENTS.md`** — the tier-0 agent entry: the core-vs-paint boundary, the
  ten non-negotiable house rules, and the docs layering, pointing at
  [[Agent-Interface]] for the four-tier map (onboarding → CLI → stdio MCP →
  network MCP). `aoide guide` prints the map at runtime; `docs/agent/`
  routes the read order.
- **`docs/BUILD.md`** — module-authoring conventions: how the catalogue names,
  the option table, how to author a dendrite or a paint lane, the checks to keep green.

## Build and verify

```
cd ~/Aoide
nix flake check                                       # both assertions + both packages build
nix build .#aoide                                # the package
nix eval .#nixosConfigurations.yomi-strix.config.system.build.toplevel.drvPath
cargo test -p aoide-cli                                # per-crate only — see below
nix build .#checks.x86_64-linux.vm-boot -L             # headless QEMU boot test
```

**Per-crate tests only.** `cargo test -p <crate>`, never `cargo test
--workspace`: `aoide-conduct`/`aoide-server` bind real sockets and a
workspace-wide run deadlocks on this machine. Each crate carries its own
`registry.rs` golden test pinning its exact command-path set (`aoide-cli`:
80 paths; `aoide-lyra`: 48), plus schema validity, exit-code, and MCP
tool-list-parity tests; the conduct crate's graph domain
(`crates/conduct/src/graph/{model,doc,common,commands,window,session_store,
conduct,send}.rs`) carries handlers for all 20 commands the bare `graph`
render, `graph link`, the `project`/`session` families, bare `session` (the
roster — grouped by project, or by host under `--hosts`), and bare `send`/`spawn`/`resurrect` register into: cycle
rejection, anchoring, a
deterministic render snapshot, edge shape, prune orphan-clearing,
unknown-field round-trip, a serialized stage-dir precedence test, and the
pure focus-liveness helpers `normalize_addr`/`window_present`.
One noted hazard: the env-var test mutex in `shellbridge.rs` is module-local
— fine while each domain crate coordinates its own env-touching tests via
`aoide-test-support::env_lock()`.

`pkgs/aoide` is a 13-crate workspace across two binaries — see
[[Package-Layout]] for the full crate roster, per-crate charter, and the
two-binary split.

## Walking-skeleton status — real vs stubbed

**Real code paths:** the whole flake/walker/option/checks layer; both packages
build; `aoide guide`, `aoide schema --json`, `mcp serve --stdio`, and the audit
log; `aoide onboard`/`lyra onboard` (the clone-onboarding lane,
[[Clone-and-Run]]); `rice lint` (native [[livery]] lint); the daemon skeleton (audit
append, user gate, default-deny event bus); shellbridge (atomic writer, seeded
stage files, and a live socket accept loop — `focuswindow`); the melete-adapter skeleton (env-driven
subscription, metadata-only notification boundary); all four livery emitters; the
QML shell skeleton; the baked Stylix and compositor fan-outs; and the whole
graph/session/project surface — 20 commands (bare `graph`, `graph link`,
`project add/remove/list`, `session start/phase/end/hook/grant/permit/
pending list/approve/deny/prune/reap`, bare `session` (the roster),
bare `send`/`spawn`/`resurrect`),
none a stub (see
[[Session-Graph]]) — plus the separate
`aoide conductor` command (also real; the liveness-reap predicate now lives in
its own `reap.rs` module, split out of `graph.rs`). `pkgs.aoide` carries unit
tests across the crate (above). The QML tree's non-stub surfaces: the
[[Gadget-Dock]] files (`AoidePanel.qml`, `GadgetFrame.qml`,
`ConductorGadget.qml`, `TerminalsGadget.qml`, `MetersGadget.qml`,
`PowerVitalsGadget.qml`), `AoideLauncher.qml` (the launcher), and
`AoideNotifications.qml` (the notification daemon — per-card body from the
song's `widgets/notifications.qml`) —
across the lyra lane's nine `owner = "quickshell"` surfaces (bar, notifications,
launcher, osd, lockscreen, greeter, wallpaper, agentWidgets, sessionGraph; see
[[Full-Architecture]]). `sessionGraph` is declared but has no QML body today
— the DAG renders via bare `aoide graph`/`aoide conductor`, not a desktop
overlay ([[Session-Graph]]). The bootable yomi-strix profile and the
vm-boot check (above) are likewise real.

**The dendrite set** now spans 36 catalogue entries in `modules/default.nix`: bash
(the `ad*` nh alias family replacing `dx*`), nh, git, kitty, neovim-via-nvf,
starship, mcfly, btop, yazi, fastfetch, devtools, cli, fonts, hyprland,
obsidian, melete, mneme, firefox, screenshot, vision, audio, claude-code,
clipboard, dunst, kimi-code, networkmanager, and pi-coding-agent — the first
twenty ported from [[dxflake]]'s prior rig into Aoide shape, the rest grown
since. `hyprland` is the host-invariant
half of the compositor: keybinds, input devices, tiling layout, misc, and
behavioural window rules, split out so a re-rice cannot disturb them (the
compositor lane keeps the livery-derived look and the session plumbing). The `nvf` flake input threads to home-manager via
`extraSpecialArgs`; the cover-art token (`aoide.livery.wallpaper` → the shared
`song/covers/`) backs the shipped wallpapers; Lekton Nerd Font Mono is the stylix
face. `neovim`'s own `vim.extraPackages` (nvf) also carries `pkgs.rustc`/
`pkgs.cargo`, scoped to nvim's wrapped PATH only — its built-in rust-analyzer
`root_dir` detection shells out to `rustc` directly and needs it, while the rest
of the system keeps the rust toolchain devShell-only. The [[Gadget-Dock]]
carries the waybar-homage bar rework plus
`NowPlaying/Power/Calendar gadgets. Baseline dendrites default on through the
`base` and `desktop` aggregations via `mkDefault`; `allowUnfree` is carried
mkIf-scoped by the two
dendrites that need it (devtools, fonts).

**Structured not-implemented stubs (exit 64, 9 total):** the mutating CLI
commands — `rice declare/transpose`, the five-command `content` pipeline, `make`,
`update`. Their arg-parsing, schema, gate flag, and audit trail are
real; only the live-system action is deferred. (`rice stage`/`rice compose`/
the `rice draft` group are **real** — `rice stage` stages
`song/stage/livery.json` for Quickshell hot-reload, applies geometry,
borders and the hyprglass switch over `hyprctl`, and recolours kitty through
`stage/terminal-colors.conf` and kitty's control socket. There is
no `rice gen` — a speculative prompt/wallpaper generator that was cut
outright rather than left as a stub with no design behind it.)

## Related

- [[Plugin-Architecture]] — the discovery-by-existing rule the catalogue's one line per capability implements
- [[Snowflake-Anatomy]]
- [[Full-Architecture]]
- [[Package-Layout]]
- [[Session-Graph]]
- [[aoide-cli]]
- [[livery]]
- [[aoided]]
- [[shellbridge]]
- [[dxflake]]
- [[Governance]]
- [[Gadget-Dock]]
- [[Rebuild-Gate]]
