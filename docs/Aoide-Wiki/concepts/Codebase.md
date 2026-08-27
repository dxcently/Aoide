---
type: concept
created: 2026-07-26
updated: 2026-08-27
tags: [aoide, architecture, nix, flake, rust, node]
---

# Codebase — How the Built Repo Works

The implementation of Aoide at `~/Aoide`, the walking-skeleton milestone: the
flake, the walker, the option contract, the systemd/service map, the runtime
contracts, and what is real versus stubbed. It is the map from the design
concepts ([[Snowflake-Anatomy]], [[Full-Architecture]]) to the files on disk.

## The flake

`flake.nix` is the fixed skeleton — later waves ADD files in their own dirs and
never touch it. Inputs: `nixpkgs` (unstable), `home-manager`, `stylix`,
`quickshell`, `hyprland` (the last four all `follows` nixpkgs where they can).
Outputs, all tolerant of empty layers so eval stays robust:

- `nixosConfigurations.yomi-strix` — assembled by `lib/mkHost.nix`.
- `packages` — **auto-discovered** by `lib/pkgs.nix` from `pkgs/<name>/default.nix`
  (`callPackage`, `_`-shelving); currently `{aoide, hyprglass, kimi-code,
  melete, mneme}` plus `default` (= aoide). Adding a package is one folder —
  this file never changes.
- `checks` — the three coupling assertions, one auto-generated `pkg-<name>` per
  discovered package, plus the `vm-boot` headless boot test (below).
- `devShells.default` — Rust (cargo/rustc/clippy/rust-analyzer) + nix
  tooling (nixfmt/nil/deadnix/statix).
- `formatter` — nixfmt.

## The walker and host assembly (`lib/`)

**`lib/walk.nix`** is the dendritic walker: `lib.filesystem.listFilesRecursive`
over a directory, filtered to `.nix` files whose path does not contain `/_`.
That single infix rule is the **shelving opt-out** — prefix a file or directory
with `_` (`_example.nix`, `_scratch/`) to hide it from discovery without
deleting it. There is no import list; a file placed under a walked directory
self-registers. This is [[dxflake]]'s pattern, now implemented in-house, and
the concrete case of [[Plugin-Architecture]]'s discovery-by-existing rule.

**`lib/pkgs.nix`** is the same idea for `pkgs/`: it reads `../pkgs`, keeps each
directory without a `_` prefix that holds a `default.nix`, and maps it to
`callPackage`. One source feeds the flake `packages` output, the host + vm
overlays, and the `pkg-<name>` checks — so a package self-registers everywhere
from a single new folder. Its `overlay` form guards each name against
accidentally masking a nixpkgs attribute; a deliberate shadow is listed in
`intentionalShadows`, and a non-standard build arg goes through a documented
`//` escape hatch (kept in this file to preserve the single source).

**`lib/mkHost.nix`** assembles one host's `nixosSystem`: it walks `../modules`
(the whole snowflake — nucleus + dendrites + facets) **and** `song/songbook/`
— so songs self-register exactly like dendrites. It then appends the host's own
dir, home-manager and stylix (each added only when its input is present, so a
minimal eval still works), and the **discovered-packages overlay** from
`lib/pkgs.nix` — the *same* source the flake's `packages` output and the
`pkg-<name>` checks read, so nucleus/facet modules reference `pkgs.aoide` /
… without drift. The overlay guards each name against
accidentally masking a nixpkgs attribute (a deliberate shadow — Aoide's `melete`
harness vs the nixpkgs `melete` font — is an `intentionalShadows` exemption). It passes `host`, `inputs`, `username`, `system` as
`specialArgs`. Each song's `rice.nix` guards itself with
`lib.mkIf (config.aoide.song == "<name>")`, so committing a song makes it
fleet-available and a single `aoide.song` declaration in `hosts/<host>/default.nix`
selects which one a host performs (see [[Song-Vocabulary#Replay — any song, any host]]).

**`lib/songbook.nix`** is the songbook manifest/registry generator (the one
generator two callers share: the quickshell facet's build-time `manifest.json`/
`registry.json` derivation, and the `songbookManifest` flake output
[[Self-Ricing|`rice stage`]] shells out to via `nix eval --json` for its
hot-sync half). **`lib/livery.nix`** resolves `aoide.livery.override.*`, the
host-set venue-recolour tier (`CONTRACTS.md` §1). **`lib/song.nix`** builds
each song's widget records from its `_widgets/<slot>.nix` functions — the nix
analogue of `WidgetSlot.qml`.

**`lib/checks.nix`** rides as flake `checks` — the contractual coupling
discipline, written to throw legibly at eval time on a violation:

- **`surface-ownership`** — every declared `aoide.surfaces.<name>` names a
  non-empty owner (a facet asserting it is the sole owner of a render surface).
- **`no-song-read`** — no walked module path lives under a `song/` runtime dir
  (`stage/`, `auditions/`, `catalog/`, `index/`), so the nix build
  can never come to depend on ephemeral runtime state. `stage/` stays non-load-
  bearing structurally. This boundary is about runtime dirs only; committed
  `song/songbook/**` is versioned score and is legitimately read at eval.
- **`song-shape`** — every walked `song/songbook/**` path is a `rice.nix`
  (the host-agnostic song discipline, `CONTRACTS.md §5`).

Alongside these, `flake.nix` auto-generates a `pkg-<name>` check per discovered
package (from `lib/pkgs.nix`) so every package builds under `nix flake check`.

**`lib/vmTest.nix`** wires `checks.<system>.vm-boot` — a
headless QEMU boot of the whole stack via `pkgs.testers.runNixOSTest`
(4 GiB / 4 vCPU, KVM). Its node is assembled from the **same** parts
`mkHost.nix` uses — the walked module tree, the songbook walk, the
home-manager/stylix modules, the pkgs overlay, and mirrored `specialArgs`
(`host = "vm-test"`, inputs, username, system; `node.pkgsReadOnly = false`
so the overlay applies) — so the test boots the real assembly, not a
replica. It asserts: `multi-user.target` reached; `aoide` on
PATH with `schema --json` reporting exactly 73 commands (a hardcoded
drift-tripwire figure, [[AOIDE-DEV]] §7) and `guide` exiting
0; greetd enabled (a Hyprland respawn loop on the virtual GPU is tolerated);
linger active with the `aoided` and `shellbridge` user units finishing
`Result=success` (the skeleton binaries seed state and exit 0); stage files
seeded at `schemaVersion` 0; and a `project add → graph` round-trip landing
`graph.json` (every stage mutation restages it automatically, so there is no
separate emit step to round-trip through). The test node trims the stylix and quickshell facets
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
- `aoide.song` (str, default `"sonata"`) — which song this host performs.
  Set once in `hosts/<host>/default.nix`; each song's `rice.nix` guards itself
  with `lib.mkIf (config.aoide.song == "<name>")`. See [[Song-Vocabulary#Replay — any song, any host]].
- `aoide.livery` — the v0 livery schema: closed `palette.{bg,fg,accent,urgent}`
  (base16, permissive hex type) + optional component tiers `bar.*` / `notif.*` /
  `window.*` (each `nullOr` hex, `null` → palette), plus an `override.*` venue
  tier (host-set only, [[livery|resolved by `lib/livery.nix`]]).
- `aoide.arrangement` — the v1 arrangement schema: which widget/surface types
  a song brings into existence. `aoide.livery` and `aoide.arrangement` are the
  whole facet-read whitelist — no module reads another module's options.
- `aoide.surfaces.<name>.owner` — the surface-ownership registry.
- `aoide.mcp.enable`, `aoide.a2a.enable`, `aoide.usage.enable`,
  `aoide.secrets.enable` — each off by default (house policy); `aoide.auditLog`
  (default `/home/<user>/Aoide/log`).

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
  unified 32 GB pool); amdgpu userspace graphics for the Hyprland facet;
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
| `shellbridge` | `nucleus/shellbridge.nix` | runs `lyra shellbridge --run`; `RuntimeDirectory=aoide` for the socket |
| `aoide-graph-reap` | `nucleus/shellbridge.nix` | timer, ~12s interval; the liveness reaper sweeping sessions a killed terminal could never mark `done` |
| `aoide-melete-adapter` | `nucleus/melete-adapter.nix` | runs `aoide adapter melete --run`; `AOIDE_ADAPTER_SUBSCRIBE` allow-list |
| `aoide-mcp` | `nucleus/aoided.nix` | **gated on `aoide.mcp.enable`**; `bindsTo` aoided |
| `aoide-a2a` | `nucleus/aoided.nix` | **gated on `aoide.a2a.enable`**; the [[A2A-Door]] serve unit |
| `aoide-usage` + timer | `nucleus/aoided.nix` | **gated on `aoide.usage.enable`**; runs `aoide usage` on `aoide.usage.interval` |
| `aoide-secrets-serve` | `nucleus/secrets.nix` | **gated on `aoide.secrets.enable`**; SYSTEM (not user) service, own uid `aoide-secrets`, anchored to `multi-user.target` — see [[Secrets-Broker]] |
| `aoide-obsidian-register` | `dendrites/obsidian.nix` | oneshot; registers a window class with shellbridge |

`systemd.user.tmpfiles.rules` create `~/Aoide/log` (0700), `~/Aoide/state/stage`
(0755, conducting state), and `~/Aoide/song/stage` (0755, rice staging) at
runtime; systemd-tmpfiles deduplicates the shared rules declared in both
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

Live-side state, all gitignored, none load-bearing for the build:

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
  `home.activation.aoideSeedStage` in `modules/facets/quickshell/default.nix`
  — a write-temp-then-rename script that injects the same `"song"` field
  `rice stage` writes, so a host that boots without ever staging still
  carries a correct live stage twin from the baked default), `mode.json`
  (the staging/declarative mode marker — absent reads as `declarative`), and
  `grimoire.json` (QML-only writer). `state/stage/` holds CONDUCTING
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
  own default location under `~/Aoide/` — the documented CLI ↔ unit seam,
  pinned by a serialized precedence test. The first no-override resolution
  of `conducting_stage_dir()` runs a one-shot migration moving any of the
  six conducting files still sitting at the old `song/stage/` location into
  `state/stage/`, never clobbering a fresher file already there and never
  touching a rice file.

## Repo-file roles

- **`CONTRACTS.md`** — §0 design philosophy plus the versioned contracts §1–8:
  note schema, dendrite shape, `aoide schema --json` output, stage-file
  formats, song shape, A2A door, peer federation, screen capture + pointer
  synthesis. The `checks` fail a merge that breaks one; bumping a version
  needs a playbook migration.
- **`AGENTS.md`** — the tier-0 agent entry: the core-vs-paint boundary, the
  nine non-negotiable house rules, and the docs layering, pointing at
  [[Agent-Interface]] for the four-tier map (onboarding → CLI → stdio MCP →
  network MCP). `aoide guide` prints the map at runtime; `docs/agent/`
  routes the read order.
- **`docs/BUILD.md`** — module-authoring conventions: how the walker discovers,
  the option table, how to author a dendrite/facet, the checks to keep green.

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
73 paths; `aoide-lyra`: 43), plus schema validity, exit-code, and MCP
tool-list-parity tests; the conduct crate's graph domain
(`crates/conduct/src/graph/{model,doc,common,commands,window,session_store,
conduct,send}.rs`) carries handlers for all 19 commands the bare `graph`
render, `graph link`, the `project`/`session` families, and bare
`send`/`spawn`/`resurrect` register into: cycle rejection, anchoring, a
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
graph/session/project surface — 19 commands (bare `graph`, `graph link`,
`project add/remove/list`, `session start/phase/end/hook/carry/permit/
pending list/approve/deny/prune/reap`, bare `send`/`spawn`/`resurrect`),
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
across the facet's nine `owner = "quickshell"` surfaces (bar, notifications,
launcher, osd, lockscreen, greeter, wallpaper, agentWidgets, sessionGraph; see
[[Full-Architecture]]). `sessionGraph` is declared but has no QML body today
— the DAG renders via bare `aoide graph`/`aoide conductor`, not a desktop
overlay ([[Session-Graph]]). The bootable yomi-strix profile and the
vm-boot check (above) are likewise real.

**The dendrite set** now spans 27 entries in `modules/dendrites/`: bash
(the `ad*` nh alias family replacing `dx*`), nh, git, kitty, neovim-via-nvf,
starship, mcfly, btop, yazi, fastfetch, devtools, cli, fonts, hyprland,
obsidian, melete, mneme, firefox, screenshot, vision, audio, claude-code,
clipboard, dunst, kimi-code, networkmanager, and pi-coding-agent — the first
twenty ported from [[dxflake]]'s prior rig into Aoide shape, the rest grown
since. `hyprland` is the host-invariant
half of the compositor: keybinds, input devices, tiling layout, misc, and
behavioural window rules, split out so a re-rice cannot disturb them (the
compositor facet keeps the livery-derived look and the session plumbing). The `nvf` flake input threads to home-manager via
`extraSpecialArgs`; the cover-art token (`aoide.livery.wallpaper` → the shared
`song/covers/`) backs the shipped wallpapers; Lekton Nerd Font Mono is the stylix
face. `neovim`'s own `vim.extraPackages` (nvf) also carries `pkgs.rustc`/
`pkgs.cargo`, scoped to nvim's wrapped PATH only — its built-in rust-analyzer
`root_dir` detection shells out to `rustc` directly and needs it, while the rest
of the system keeps the rust toolchain devShell-only. The [[Gadget-Dock]]
carries the waybar-homage bar rework plus
NowPlaying/Power/Calendar gadgets. Baseline dendrites default on in
`hosts/common` via `mkDefault`; `allowUnfree` is carried mkIf-scoped by the two
dendrites that need it (devtools, fonts).

**Structured not-implemented stubs (exit 64, 9 total):** the mutating CLI
commands — `rice declare/transpose`, the five-command `content` pipeline, `make`,
`update`. Their arg-parsing, schema, gate flag, and audit trail are
real; only the live-system action is deferred. (`rice stage`/`rice compose`/
the `rice draft` group are **real** — `rice stage` stages
`song/stage/livery.json` for Quickshell hot-reload today; only the
hyprctl/OSC dispatch fan-out remains unwired into `stage` itself. There is
no `rice gen` — a speculative prompt/wallpaper generator that was cut
outright rather than left as a stub with no design behind it.)

## Related

- [[Plugin-Architecture]] — the discovery-by-existing rule `lib/walk.nix` implements
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
