# Aoide

**Aoide (the core) vs AoideOS (the distribution) — don't conflate the two.** *Aoide* tracks and conducts terminal and agent sessions. Sessions collaborate across agents and across hosts, with the human in the loop — and become the parts you build larger systems out of. Any agent with a shell is fully capable, no MCP required; **every terminal is a conductable, tracked session by default**, so a central agent can speak into any other running session (see [Conducting](#conducting--every-terminal-is-a-tracked-session)). The core runs anywhere there is a shell — portable, headless-capable, agent-first. *AoideOS* is the **distribution built on that core**: the NixOS flake that ADDITIONALLY ships the Quickshell widget-making toolkit (bar, dock, gadgets, the DAG/conductor surfaces) and the specialized ricer (the song/notes theming engine). Aoide is the engine; AoideOS is the desktop around it — a capability that works with only a shell is "Aoide", one that is desktop/Quickshell/rice is "AoideOS".

**AoideOS** is the OS harness for your harness — Hyprland compositor, Quickshell shell, an orchestrator daemon (`aoided`), a content pipeline, and a self-ricing engine — built on the shell-only Aoide core, that you **clone and run**, not install. Its naming thesis in one line: architecture is frozen music — the nix layer is the score, the running desktop is the performance, a rice is a song the system sings. The Aoide·Melete·Mneme naming is a three-Muses theme: Aoide is the *song* (this repo), [Melete](#melete--the-doer) is *practice* (the coding harness), [Mneme](#mneme--the-door) is *memory* (the knowledge vault) — the other two are independently-owned systems Aoide **integrates and launches**, never vendors.

> Status — walking skeleton. The desktop, the flake, the `graph` group, the daemon, and the theming fan-out are **real and running live** (yomi-strix is switched onto this flake). A handful of commands (`rice declare`/`transpose`, the `content` group, `make`, `update`, `onboard`) are **structured stubs** that return exit `64` (`not-implemented`) with the right shape — the trunk is wired, the muscle is being grown. See [`aoide-cli`](docs/Aoide-Wiki/entities/aoide-cli.md) for exactly which.

This README explains what Aoide/AoideOS *is* — architecture, features, the Melete/Mneme integration, and how to install it. For day-to-day driving (every keybind, alias, and CLI command) see the linked wiki pages in [§6](#6-further-documentation) — nothing here duplicates a reference table that already lives there.

## Contents

1. [Architecture](#1-architecture)
2. [Features](#2-features)
3. [Melete & Mneme](#3-melete--mneme)
4. [Install (Nix flakes)](#4-install-nix-flakes)
5. [How Aoide updates](#5-how-aoide-updates)
6. [Further documentation](#6-further-documentation)

---

## 1. Architecture

The repo is a **snowflake**: everything lives under `modules/`, walked and self-registered by an in-house dendritic walker (`lib/walk.nix` + `lib/mkHost.nix`). No import lists — drop a `.nix` file in the right directory and it registers itself. A `/_`-prefixed path (`_wip.nix`, `_scratch/`) is **hidden** from the walker.

```
~/Aoide/
├── modules/
│   ├── nucleus/    core: aoided daemon, shellbridge, CLI packaging, options, policy
│   ├── dendrites/  opt-in features — one tree, shipped + personal branches
│   └── facets/     render surfaces (read ONLY aoide.livery): quickshell · compositor · stylix
├── hosts/
│   ├── common/     cross-machine baseline (which dendrites default ON)
│   └── <host>/     machine-specific picks (hardware, enabled facets, song)
├── pkgs/           the aoide CLI + daemon (Rust, its own flake, consumed as a path input)
├── lib/            the walker + mkHost + checks
├── song/           the performed half (rices, songbook, runtime stage/)
└── flake.nix       inputs + outputs (never edited to add a module)
```

Three layers, radial distance from the nucleus governing who may change what — see [Snowflake Anatomy](docs/Aoide-Wiki/concepts/Snowflake-Anatomy.md) for the full mutation policy:

| Layer | Owner | What it holds |
|---|---|---|
| `modules/nucleus/` | upstream | The daemon, CLI packaging, the option contract, policy. |
| `modules/dendrites/` | shipped + you | Opt-in features, one flat tree; new personal dendrites are additive files. |
| `modules/facets/` | upstream | Render surfaces — Quickshell, the compositor, Stylix — reading only `aoide.livery`. |
| `song/` | you (agent-written) | The performed half: committed songs, palettes, and the gitignored runtime `stage/`. |

Deeper reads:

- [Full Architecture](docs/Aoide-Wiki/concepts/Full-Architecture.md) — the whole-body map: every subsystem, its inputs/outputs, where the frozen and performed halves meet at the livery seam.
- [Codebase](docs/Aoide-Wiki/concepts/Codebase.md) — how the built repo actually works: the flake, the walker + overlay, the option contract, the systemd/service map, socket + stage-file contracts, real vs stubbed.
- [Desktop Architecture](docs/Aoide-Wiki/concepts/desktop/Desktop-Architecture.md) — how `aoided`, `shellbridge`, Quickshell, and the compositor compose into one agent-ready desktop body.
- [Package Layout](docs/Aoide-Wiki/concepts/Package-Layout.md) — the `pkgs/aoide` Rust crate split (protocol, storage, conduct, song, cli, …) and its phased migration.
- [Song Anatomy](docs/Aoide-Wiki/concepts/song/Song-Anatomy.md) / [Song Vocabulary](docs/Aoide-Wiki/concepts/song/Song-Vocabulary.md) — the performed half's own tree and naming map.

---

## 2. Features

### Self-ricing

The headline feature. An agent generates, lints, stages (hot-loads live, nothing committed), and adopts a **rice** — a song the system performs, drawn from a wallpaper or a prompt. Committed songs live in `song/songbook/<name>/`; any host performs any committed song by naming it (`aoide.song = "moonlight";`). Staging vs declarative mode lets an agent hot-iterate on a rice live without ever touching the committed, home-manager-declared state until an explicit `adopt`. → [Self-Ricing](docs/Aoide-Wiki/concepts/song/Self-Ricing.md), [Ricing Protocol](docs/Aoide-Wiki/concepts/song/Ricing-Protocol.md).

### Conducting — every terminal is a tracked session

Every terminal runs its login shell under `aoide conduct` by default, so it registers in a live session DAG **and** holds a control socket a central conductor can type into. `aoide send` injects text into another session's stdin, gated pending-by-default (delivers on `--yes`, an autogate switch, or when the sender is the target's own parent). `aoide conductor` is the interactive terminal UI over the same trunk — session roster, projects, audit log, stage health. → [Conductor Channel](docs/Aoide-Wiki/concepts/orchestration/Conductor-Channel.md), [Session Graph](docs/Aoide-Wiki/concepts/orchestration/Session-Graph.md), [Terminal Commander](docs/Aoide-Wiki/concepts/orchestration/Terminal-Commander.md).

### Pairing — how conducting reaches another host

Two instances become nodes through a mutual, two-code ceremony, named here by seat: the requester runs `aoide pair <target>`, raising a request; the approver runs `aoide pair <id>`, typing the code read off the requester's screen, and its own node record commits — its screen then shows a second, different reply code; the requester's still-running `aoide pair` types that reply code, and its own record commits last. Both legs are typed, never a bare yes/no, gated identically whether driven from a plain terminal or `aoide pair watch --popup`'s Quickshell dialogs (`lyra pair ask` to type, `lyra pair show` to display). What it buys: a verified node record carrying an `allows` set (`read`, `spawn`), a cross-host `node list` roster with `node pull`/`node spawn` reaching into it, and — for a node whose own door binds loopback-only — a reach-back over an ssh `via` hop, never a routable bind. → [Pairing Ceremony](docs/Aoide-Wiki/concepts/orchestration/Pairing-Ceremony.md), [Doors & Nodes](docs/Aoide-Wiki/concepts/cli/Doors-and-Nodes.md).

### Widget-maker

Because [Melete](#melete--the-doer) writes code, you extend AoideOS by asking it to *generate* a new integration — a dendrite (nix) + widget (QML) + adapter — rather than hunting for a plugin. The same gated stage → adopt loop as self-ricing governs what lands. → [Widget Maker](docs/Aoide-Wiki/concepts/desktop/Widget-Maker.md), [Feature Set](docs/Aoide-Wiki/concepts/desktop/Feature-Set.md).

### Content pipeline

A discover → propose → **approve** → ingest → lint → query pipeline over content sources, with a non-negotiable human approve gate and a quarantine branch for anything that fails lint after ingest. Reads a [Mneme](#mneme--the-door) vault exclusively through its API — raw filesystem access is never permitted. → [Content Pipeline](docs/Aoide-Wiki/concepts/orchestration/Content-Pipeline.md).

### A2A door

A third, optional door alongside the CLI and stdio MCP: a bidirectional Agent2Agent (JSON-RPC/HTTP) interop wire — Aoide as a discoverable A2A agent (server), and an A2A client that drives external agents, all from the one command registry. → [A2A Door](docs/Aoide-Wiki/concepts/orchestration/A2A-Door.md).

### Governance & the rebuild gate

Every operation flows through `aoided` — one policy surface, one gate, one audit log. The one carefully-gated step in every loop above is the rebuild itself: agents build, dry-activate, and test freely (no privilege needed), but making a change stick stops and prompts the human to `switch` under their own `sudo`. NixOS rollback backs every switch. → [Governance](docs/Aoide-Wiki/concepts/governance/Governance.md), [Rebuild Gate](docs/Aoide-Wiki/concepts/governance/Rebuild-Gate.md).

---

## 3. Melete & Mneme

Aoide **integrates and launches** two independently-owned systems — the repo ships only their launchers (`modules/dendrites/{melete,mneme}.nix`) and the adapter that translates Aoide's neutral event stream into their actions, never their source.

### Melete — the doer

A long-running coding harness: dispatches autonomous coding runs, executes shell commands, drives a headless `claude` CLI, reaches out to GitHub and a rented fleet. It's the engine behind [widget-making](#widget-maker) — because Melete writes code, new integrations are *generated*, not installed. The relationship runs both ways: `melete aoide …` routes back into this CLI trunk, so Melete can drive Aoide too. A `melete-adapter` systemd unit consumes `aoided`'s neutral event stream under a default-deny subscription; forwarded notification text always reaches it as untrusted metadata, never a command. → [Melete](docs/Aoide-Wiki/entities/Melete.md).

### Mneme — the door

An independently-owned vault API serving a folder of notes over MCP — read, write, snapshot, search, versioned, with trash/recovery. It's the substrate behind the [content pipeline](#content-pipeline) and the [Wiki Protocol](docs/Aoide-Wiki/concepts/governance/Wiki-Protocol.md) (every project — Aoide included — gets a standalone wiki in a shared shape). Where Melete has a shell and writes raw, Mneme is the narrow authenticated API for the shell-less reader — a deliberate injection-defense seam, not an arbitrary split. → [Mneme](docs/Aoide-Wiki/entities/Mneme.md).

---

## 4. Install (Nix flakes)

Aoide is a framework you **clone and run**, not a package you install — the upstream repo ships the shape-making machinery (engine, contracts, walker, facets) but never the shapes themselves. Your clone is your instance, and shared git history means upstream improvements arrive as an ordinary merge. A remote fork is optional — for backup, fleet sync, or contributing back.

**Prerequisites:** a NixOS box with flakes enabled (`nix.settings.experimental-features = [ "nix-command" "flakes" ];` in your existing config, or `experimental-features = nix-command flakes` in `/etc/nix/nix.conf`).

```sh
# 1. Clone upstream (add your own remote later only if you want one)
git clone <upstream-url> ~/Aoide
cd ~/Aoide

# 2. Add a host: one line in flake.nix's nixosConfigurations, e.g.
#      nixosConfigurations.<host> = mkHost "<host>";
#    then create hosts/<host>/default.nix importing hosts/common —
#    start from a shelved skeleton (hosts/_desktop, _laptop, or _server;
#    _mac is the forward-looking darwin one, pending the mkHost class
#    seam) or copy hosts/yomi-strix/, the living reference. A committed
#    hardware.nix is imported guardedly if present.

# 3. Build + switch
sudo nixos-rebuild switch --flake .#<host>
```

From then on the flake ships its own rebuild aliases (`adbuild`/`adtest`/`adrebuild`/…) for the everyday loop — see [Controls](docs/Aoide-Wiki/concepts/desktop/Controls.md). `aoide onboard` is the specified one-shot version of steps 2–3 (generate the host, seed `song/`, print the agent guide) but is a stub today (exit `64`) — drive it by hand with the steps above meanwhile. Full onboarding narrative, done-state checks, and self-update: [Clone and Run](docs/Aoide-Wiki/concepts/governance/Clone-and-Run.md).

---

## 5. How Aoide updates

Two axes move independently, and neither has a background updater — house policy routes both through the gated rebuild.

- **Dependency versions** are yours to bump on any schedule: `adupdate` (`nh os switch --update`) rewrites `flake.lock` and switches in one step.
- **The framework itself** is upstream's shape, pulled in by merge: `git fetch upstream && git merge upstream/main`, then `adcheck && adrebuild`. `aoide update` is the eventual guided path for this (fetch, merge framework paths, run checks, detect contract bumps, propose the rebuild) — a stub today.

→ [Clone and Run § Self-Update](docs/Aoide-Wiki/concepts/governance/Clone-and-Run.md#self-update), [Governance](docs/Aoide-Wiki/concepts/governance/Governance.md).

---

## 6. Further documentation

| Doc | What's there |
|---|---|
| [`docs/Aoide-Wiki/Overview.md`](docs/Aoide-Wiki/Overview.md) | The wiki's own clickable index — every concept and entity page, start here for depth. |
| [`docs/Aoide-Wiki/entities/aoide-cli.md`](docs/Aoide-Wiki/entities/aoide-cli.md) | The full `aoide` command tree, real vs stub, contract-level conventions. |
| [`docs/Aoide-Wiki/concepts/desktop/Controls.md`](docs/Aoide-Wiki/concepts/desktop/Controls.md) | Every keybind and shell alias: the `ad*` rebuild family, compositor keybinds, bar interactions, shell QoL aliases. |
| [`AGENTS.md`](AGENTS.md) | The agent-facing onboarding doc — house rules and pointers, for any agent driving this repo; `docs/agent/` is its checkout-side router. |
| [`CONTRACTS.md`](CONTRACTS.md) | Versioned interfaces: the note schema, dendrite shape, `schema --json`, stage-file formats. |
| [`docs/BUILD.md`](docs/BUILD.md) | Module-authoring: how to write a dendrite or facet. |
