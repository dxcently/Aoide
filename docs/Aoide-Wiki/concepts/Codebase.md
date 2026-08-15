---
type: concept
created: 2026-07-26
updated: 2026-08-14
tags: [aoide, architecture, nix, flake, rust, node]
---

# Codebase — How the Built Repo Works

The implementation of Aoide at `~/Aoide`, the walking-skeleton milestone: the
flake, the walker, the option contract, the systemd/service map, the runtime
contracts, and what is real versus stubbed. It is the map from the design
concepts ([[Snowflake-Anatomy]], [[Full-Architecture]]) to the files on disk.

*Everything verifies green (cargo tests, flake check, vm-boot), and the
profile **runs live** on yomi-strix — the graphical session (greetd →
Hyprland → Quickshell) included.*

## The flake

`flake.nix` is the fixed skeleton — later waves ADD files in their own dirs and
never touch it. Inputs: `nixpkgs` (unstable), `home-manager`, `stylix`,
`quickshell`, `hyprland` (the last four all `follows` nixpkgs where they can).
Outputs, all tolerant of empty layers so eval stays robust:

- `nixosConfigurations.yomi-strix` — assembled by `lib/mkHost.nix`.
- `packages` — **auto-discovered** by `lib/pkgs.nix` from `pkgs/<name>/default.nix`
  (`callPackage`, `_`-shelving); currently `{aoide, melete, mneme}` plus
  `default` (= aoide). Adding a package is one folder — this file never changes.
- `checks` — the three coupling assertions, one auto-generated `pkg-<name>` per
  discovered package (`pkg-aoide`/`pkg-melete`/`pkg-mneme`), plus
  the `vm-boot` headless boot test (below).
- `devShells.default` — Rust (cargo/rustc/clippy/rust-analyzer) + nix
  tooling (nixfmt/nil/deadnix/statix).
- `formatter` — nixfmt.

## The walker and host assembly (`lib/`)

**`lib/walk.nix`** is the dendritic walker: `lib.filesystem.listFilesRecursive`
over a directory, filtered to `.nix` files whose path does not contain `/_`.
That single infix rule is the **shelving opt-out** — prefix a file or directory
with `_` (`_example.nix`, `_scratch/`) to hide it from discovery without
deleting it. There is no import list; a file placed under a walked directory
self-registers. This is [[dxflake]]'s pattern, now implemented in-house.

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
PATH with `schema --json` reporting exactly 54 commands (a hardcoded
drift-tripwire figure, [[AOIDE-DEV]] §7) and `guide` exiting
0; greetd enabled (a Hyprland respawn loop on the virtual GPU is tolerated);
linger active with the `aoided` and `shellbridge` user units finishing
`Result=success` (the skeleton binaries seed state and exit 0); stage files
seeded at `schemaVersion` 0; and a `graph project add → view → emit` round-trip
landing `graph.json`. The test node trims the stylix and quickshell facets
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
  `window.*` (each `nullOr` hex, `null` → palette). This is the **only** thing
  facets read.
- `aoide.surfaces.<name>.owner` — the surface-ownership registry.
- `aoide.mcp.enable` (default false — house policy), `aoide.auditLog` (default
  `/home/<user>/Aoide/log`).

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
| `shellbridge` | `nucleus/shellbridge.nix` | runs `aoide shellbridge --run`; `RuntimeDirectory=aoide` for the socket |
| `aoide-melete-adapter` | `nucleus/melete-adapter.nix` | runs `aoide adapter melete --run`; `AOIDE_ADAPTER_SUBSCRIBE` allow-list |
| `aoide-mcp` | `nucleus/aoided.nix` | **gated on `aoide.mcp.enable`**; `bindsTo` aoided |
| `aoide-obsidian-register` | `dendrites/obsidian.nix` | oneshot; registers a window class with shellbridge |

`systemd.user.tmpfiles.rules` create `~/Aoide/log` (0700) and
`~/Aoide/song/stage` (0755) at runtime; systemd-tmpfiles deduplicates the shared
rule declared in both aoided and shellbridge.

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
- **Stage files** under `song/stage/`: `livery.json` (resolved livery colours,
  written by [[livery]]'s `rice stage`/`cover set`/other emitters at
  rehearsal — both refuse under `rice mode declarative`, see
  [[Self-Ricing#Staging vs Declarative Mode]] — and reseeded from the active
  song's committed
  `song/songbook/<song>/livery.json` on every activation by
  `home.activation.aoideSeedStage` in `modules/facets/quickshell/default.nix`
  — a write-temp-then-rename script that injects the same `"song"` field
  `rice stage` writes, so a host that boots without ever staging still
  carries a correct live stage twin from the baked default), `mode.json`
  (the staging/declarative mode marker — absent reads as `declarative`),
  `sessions.json` (agent session roster, written by
  [[shellbridge]]; records may carry an additive optional `parentSessionId`),
  `hooks.json` (live Claude Code hook phases), `projects.json` (the project
  registry, kept by `aoide graph project`), and `graph.json` (the resolved
  project/session DAG, written by `aoide graph emit` for Quickshell — see
  [[Session-Graph]]). Each has a v0 shape in `CONTRACTS.md §4`; writes are
  atomic (write-temp-then-rename), and the graph rewriters round-trip unknown
  fields so concurrent writers never lose data.
- **Stage-dir resolution** (`CONTRACTS.md §4`): every stage
  reader/writer resolves the stage directory as `$AOIDE_STAGE_DIR` when set
  to an absolute path (the unit sets it; empty/relative ignored), else the
  `aoide_home()` fallback — the documented CLI ↔ unit seam, pinned by a
  serialized precedence test.

## Repo-file roles

- **`CONTRACTS.md`** — the five versioned contracts: livery schema v0, dendrite
  shape v0, `aoide schema --json` output v0, stage-file formats v0, song shape
  v0 (§5). The `checks` fail a merge that breaks one; bumping a version needs a
  playbook migration.
- **`AGENTS.md`** — the tier-0 four-tier agent guide (onboarding → CLI → stdio
  MCP → network MCP) plus the six non-negotiable house rules. `aoide guide`
  prints the same map at runtime.
- **`docs/BUILD.md`** — module-authoring conventions: how the walker discovers,
  the option table, how to author a dendrite/facet, the checks to keep green.

## Build and verify

```
cd ~/Aoide
nix flake check                                       # both assertions + both packages build
nix build .#aoide                                # the package
nix eval .#nixosConfigurations.yomi-strix.config.system.build.toplevel.drvPath
cargo test                                             # schema / dispatch / mcp unit tests
nix build .#checks.x86_64-linux.vm-boot -L             # headless QEMU boot test
```

The Rust crate carries unit tests for the schema (valid JSON, stable top-level
keys, every command carries `--json` + exit codes, unique paths), the MCP
door (tool list is one-to-one with the schema; `tools/call` dispatches into the
same handlers), and the graph domain (`pkgs/aoide/src/graph.rs` is a thin
re-export root over `graph/{model,doc,common,verbs,window,session_store,
conduct,send}.rs` plus a shared `testutil` — pure cores plus handlers for all
15 `graph` subcommands: cycle rejection, anchoring, a deterministic render
snapshot, edge shape, prune orphan-clearing, unknown-field round-trip, a
serialized stage-dir precedence test, and the pure focus-liveness helpers
`normalize_addr`/`window_present`) — 63 unit tests across the crate at last
count. The split mirrors `conductor.rs` + `conductor/`: the public
`crate::graph::*` surface `dispatch`/`reap`/`conductor`/`shellbridge` reach is
unchanged by it. One noted hazard: the env-var test mutex in `shellbridge.rs`
is module-local — fine while it is the only module with env-touching tests.

`pkgs/aoide` is **one crate today**. A target blueprint for splitting it into
pi-style single-charter crates (`protocol`, `conduct`, `server`, `client`,
`storage`, `steward`, `song`, `management`, `evals`, `conductor`,
`cli`) under a `[workspace]` is specified but not built — see
[[Package-Layout]] for the target tree, per-crate charter, and the phased
migration.

## Walking-skeleton status — real vs stubbed

**Real code paths:** the whole flake/walker/option/checks layer; both packages
build; `aoide guide`, `aoide schema --json`, `mcp serve --stdio`, and the audit
log; `rice lint` (native [[livery]] lint); the daemon skeleton (audit
append, user gate, default-deny event bus); shellbridge (atomic writer, seeded
stage files, and a live socket accept loop — `focuswindow`); the melete-adapter skeleton (env-driven
subscription, metadata-only notification boundary); all four livery emitters; the
QML shell skeleton; the baked Stylix and compositor fan-outs; and the whole
`aoide graph` group — 15 subcommands (`view`, `project add/remove/list`,
`link`, `session start/phase/end/hook`, `wrap`, `send`, `focus`, `prune`,
`reap`, `emit`), none a stub (see [[Session-Graph]]) — plus the separate
`aoide conductor` command (also real; the liveness-reap predicate now lives in
its own `reap.rs` module, split out of `graph.rs`). `pkgs.aoide` carries unit
tests across the crate (above). The QML tree's non-stub surfaces: the
[[Gadget-Dock]] files (`AoidePanel.qml`, `GadgetFrame.qml`,
`ConductorGadget.qml`, `TerminalsGadget.qml`, `MetersGadget.qml`,
`PowerVitalsGadget.qml`), `AoideLauncher.qml` (the launcher), and
`AoideNotifications.qml`/`NotificationCard.qml` (the notification daemon) —
across the facet's nine `owner = "quickshell"` surfaces (bar, notifications,
launcher, osd, lockscreen, greeter, wallpaper, agentWidgets, sessionGraph; see
[[Full-Architecture]]). `sessionGraph` is declared but has no QML body today
— the standalone DAG overlay and its shared `GraphModel.qml` are no longer
in the QML tree ([[Session-Graph]]). The bootable yomi-strix profile and the
vm-boot check (above) are likewise real.

**The dendrite set** now spans twenty entries in `modules/dendrites/`: bash
(the `ad*` nh alias family replacing `dx*`), nh, git, kitty, neovim-via-nvf,
starship, mcfly, btop, yazi, fastfetch, devtools, cli, fonts, hyprland,
obsidian, melete, mneme, firefox, screenshot, and vision — ported from
[[dxflake]]'s prior rig into Aoide shape. `hyprland` is the host-invariant
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

**Structured not-implemented stubs (exit 64, 10 total):** the mutating CLI
verbs — `rice declare/transpose`, the five-verb `content` pipeline, `make`,
`update`, `onboard`. Their arg-parsing, schema, gate flag, and audit trail are
real; only the live-system action is deferred. (`rice stage`/`rice compose`/
the `rice draft` group are **real** — `rice stage` stages
`song/stage/livery.json` for Quickshell hot-reload today; only the
hyprctl/OSC dispatch fan-out remains unwired into `stage` itself. There is
no `rice gen` — a speculative prompt/wallpaper generator that was cut
outright rather than left as a stub with no design behind it.)

## Related

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
