---
type: overview
created: 2026-07-25
updated: 2026-08-26
---

# Aoide — Overview

**Aoide** is the orchestration core: the bridges and APIs between terminal, shell, system, and OS — one interface through which any agent is orchestrated for any task, no MCP required, runnable anywhere there is a shell. Every terminal is a conductable, tracked session by default ([[Conductor-Channel]]). **AoideOS** is the distribution built on that core: this repo's NixOS flake, which additionally ships the [[Quickshell]] widget surfaces (bar, dock, gadgets, conductor views) and the song/livery ricer ([[Self-Ricing]]). A capability that works with only a shell is Aoide; one that needs desktop, Quickshell, or rice is AoideOS. This wiki documents AoideOS end to end, the flake being the core's concrete running instance.

AoideOS pairs a Hyprland compositor with a Quickshell shell, an orchestrator daemon, a content pipeline, and a self-ricing engine. Upstream ships the shape-making machinery (rice engine, contracts, walker, management tools) but never the shapes; your clone is your instance ([[Clone-and-Run]]). The frozen/performed naming thesis lives in [[Lexicon]].

AoideOS is also a specialized widget maker: it integrates and launches the independently-owned [[Melete]] coding harness and [[Mneme]] knowledge server, and new integrations are written by that agent as declarative nix + QML + adapter rather than selected from a plugin registry. See [[Widget-Maker]] and [[Feature-Set]].

**Status:** yomi-strix runs the Aoide flake at HEAD. See [[Full-Architecture]] for subsystem status and [[Codebase]] for the host profile.

## Concepts

- [[Full-Architecture]] — the whole-body map: every subsystem, its inputs/outputs, and where the frozen and performed halves meet at the livery seam
- [[Plugin-Architecture]] — the design philosophy behind every contract: a capability enters by existing at a conventional path and is removable without a trace; Quickshell only paints
- [[Codebase]] — repo composition: flake outputs, the `lib/` walker + overlay, the option contract, the systemd map, socket/stage contracts, real vs stubbed
- [[Widget-Maker]] — Aoide as an extensible, declarative widget maker; the coding agent writes new integrations instead of selecting plugins
- [[Feature-Set]] — what ships in the box (Melete + Mneme) and the exemplar features: messaging bridge, fleet management, scheduled-job widget
- [[Terminal-Commander]] — the conductor-class agent-session widget: watch terminals running agents, jump to any by click or keybind
- [[Session-Graph]] — the project/session DAG and its `aoide graph` CLI; rendered via `graph view`/`--json` and the `aoide conductor` TUI
- [[Gadget-Dock]] — `AoidePanel.qml`, a left-edge panel holding four core gadgets (Conductor, Terminals, Meters, Power) plus an opt-in Usage stele; opens on hot-edge hover or SUPER+G
- [[Controls]] — the day-to-day reference: `ad*` rebuild aliases, compositor keybinds, bar cell interactions, shell QoL aliases
- [[Lexicon]] — the whole vocabulary in one place: the three Muses, the frozen/performed split, why each word family was chosen
- [[Snowflake-Anatomy]] — the flake's structural layers (nucleus, dendrites, facets) and how the walker registers modules automatically
- [[Clone-and-Run]] — the install model: clone upstream to `~/Aoide`, run `aoide onboard`; shared history enables clean upstream merges
- [[Self-Ricing]] — the agent generates, lints, previews, and adopts rices; songbook write-back is the "self" in self-ricing
- [[Song-Vocabulary]] — the performed-half naming map: key, melody, component tier, instruments, design, songbook, cover, chimes, stage, rehearsal, recording
- [[Agent-Interface]] — the CLI-first capability surface: `aoide <cmd>`, MCP as a generated façade, guide tiers, agent-first ergonomics
- [[A2A-Door]] — aoide's third door: bidirectional Agent2Agent (JSON-RPC/HTTP) interop, a discoverable server and a client driving external agents, from one command registry
- [[Secrets-Broker]] — the credential door: a socket-only broker under its own uid, TOTP-gated resolves that park for operator approval, an age-encrypted default backend
- [[Desktop-Architecture]] — how aoided, shellbridge, Quickshell, and the compositor compose into a single agent-ready desktop
- [[Content-Pipeline]] — discover → propose → approve → ingest → lint → query, with the approve gate, quarantine branch, and Mneme integration
- [[Governance]] — the rebuild gate (polkit pattern), the single audit log, and mutation policy by radial distance from the nucleus
- [[Rebuild-Gate]] — how agents rebuild: the default propose-then-human-`switch` path and the opt-in `aoide.rebuild` passwordless-narrow polkit capability
- [[Wiki-Protocol]] — the Mneme/Melete-owned protocol giving each project a standalone wiki in a shared shape; this wiki is the self-managed exception
- [[Package-Layout]] — `pkgs/aoide` as pi-style single-charter crates and the two-binary split: `aoide`/`aoided` core vs `lyra` paint

## Entities

- [[aoide-cli]] — the `aoide` CLI trunk and the `aoided` daemon binary; `schema --json` is the single source of truth behind the MCP façade. Paint commands (`rice`, `cover`, `livery`, `screen`, …) are `lyra`'s
- [[lyra]] — the AoideOS paint binary: its own 43-command schema, its own registry and golden snapshot, the one binary allowed to depend on nix
- [[livery]] — the design-token layer and its engine: the immutable seam between nix structure and runtime rendering, native in `crates/song/src/livery/` as `lyra livery emit|resolve|lint`
- [[aoided]] — the orchestrator daemon: neutral event stream, policy, lint, audit log, gated rebuild pipeline
- [[shellbridge]] — the daemon-to-desktop bridge: atomic JSON state files out, unix-socket commands in, Hyprland IPC consumed
- [[Quickshell]] — the QML shell runtime: nine surfaces declared, five with a live QML body
- [[Hyprland]] — the Wayland compositor; Aoide's only multiplexer, driven live via hyprctl
- [[Stylix]] — base16 whole-system theming; the baked fan-out from `rice.nix` to every nix-manageable target
- [[dxflake]] — the dendritic auto-discovery flake; Aoide's prior art and adoption target for the nucleus + dendrite walker
- [[Melete]] — the integrated coding harness: autonomous coding runs, shell, GitHub, fleet, scheduling; the engine behind AoideOS's widget-making
- [[Mneme]] — the integrated knowledge server: the vault's MCP API behind the content pipeline and the wiki protocol

## Sources

Primary source for this wiki: [[references/AOIDE-HANDOFF]].
