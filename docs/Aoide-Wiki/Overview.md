---
type: overview
created: 2026-07-25
---

# Aoide — Overview

**Aoide (the core) vs AoideOS (the distribution) — don't conflate the two.** Aoide is the **orchestration core**: the bridges and APIs between the terminal, the shell, the system, and the OS — one interface through which any agent is freely orchestrated for any task, no MCP required. It runs anywhere there is a shell — portable, headless-capable, agent-first — and as of the [[Conductor-Channel|conductor channel]], **every terminal is a conductable, tracked session by default**. AoideOS is the **distribution built on that core**: this repo, the NixOS flake that ADDITIONALLY ships the [[Quickshell]] widget-making toolkit (bar, dock, gadgets, the DAG/conductor surfaces) and the specialized ricer (the song/drachma theming engine, see [[Self-Ricing]] and [[Ricing-Protocol|Ricing Protocol]]). A capability that works with only a shell is "Aoide"; one that needs the desktop/Quickshell/rice is "AoideOS". This wiki documents AoideOS end to end, since the flake is the concrete running instance of the core.

**AoideOS** is the agent-agnostic NixOS desktop distribution built on that core — Hyprland compositor, Quickshell shell, an orchestrator daemon, a content pipeline, and a self-ricing engine — that you fork and run. Upstream ships the shape-making machinery (the rice engine, contracts, walker, and management tools) but never the shapes; your fork is your instance, self-updating from upstream and self-configuring to your preferences. The naming thesis: architecture is frozen music — the nix layer is the score, the running desktop is the performance, and a rice is a song the system sings.

Beyond the desktop, **AoideOS is a specialized widget maker**: it **integrates and launches** (it does not vendor) the independently-owned [[Melete]] coding harness and [[Mneme]] knowledge server, and because that agent writes code, you extend the system by having it generate new **declarative** integrations — widgets, adapters, dendrites — rather than hunting for plugins. The Aoide·Melete·Mneme (three-Muses) naming is a *theme*; the primary agent is the claude CLI, with Melete as the coding harness you can also drive it with. See [[Widget-Maker]] and [[Feature-Set]].

As of 2026-07-26 Aoide is not just built but **running live**: yomi-strix switched onto the flake (from [[dxflake]]) through the [[Rebuild-Gate]] — see [[Full-Architecture]] for the status and [[Codebase]] for the switch detail.

## Concepts

- [[Full-Architecture]] — the whole-body map: every subsystem, its inputs/outputs, and how the frozen and performed halves meet at the drachma seam
- [[Codebase]] — how the built repo actually works: the flake, the `lib/` walker + overlay, the option contract, the systemd/service map, socket + stage-file contracts, and what is real vs stubbed at the walking-skeleton milestone
- [[Widget-Maker]] — the core thesis: Aoide as an extensible, declarative widget maker; the coding agent writes new integrations rather than selecting plugins
- [[Feature-Set]] — what ships in the box (Melete + Mneme integrated) and the exemplar features: messaging bridge, fleet management, scheduled-job widget
- [[Terminal-Commander]] — the agent-session widget (conductor-class): watch the terminals running agents and jump to any by click or keybind
- [[Session-Graph]] — the project/session DAG grown from the flat roster: `aoide graph` viewer + management (anchors + spawned edges, prune, liveness-checked focus, atomic graph.json emit) — rendered via `graph view`/`--json` and the `aoide conductor` TUI; the desktop's Conductor gadget gives the at-a-glance agent-tree view instead of a standalone DAG overlay
- [[Gadget-Dock]] — the agentWidgets surface realized: `AoidePanel.qml`, a left-edge panel holding four core gadgets (Conductor, Terminals, Meters, Power) plus an opt-in Usage stele, colours entirely from drachma — its fore-edge peeks at rest and it opens fully on hot-edge hover or SUPER+G
- [[Lexicon]] — the whole vocabulary in one place: the three original Muses (Aoide · Melete · Mneme), the frozen/performed split, why each word family was selected, and the loop that ties them together
- [[Snowflake-Anatomy]] — the layered structure of the Aoide flake: nucleus, dendrites, and facets; why Nix's snowflake logo maps to the repo's growth model
- [[Fork-and-Run]] — the install model: fork upstream, clone to `~/Aoide`, run `aoide onboard`; shared history enables clean upstream merges and upstream contributions
- [[Self-Ricing]] — the headline feature: the agent generates, lints, previews, and adopts rices; songbook write-back is the "self" in self-ricing
- [[Song-Vocabulary]] — the performed-half naming map: key, melody, arrangement, instruments, design, songbook, cover, chimes, stage, rehearsal, recording
- [[Agent-Interface]] — the CLI-first capability surface: `aoide <cmd>`, MCP as a generated façade, guide tiers, and agent-first ergonomics
- [[A2A-Door]] — aoide's third door: the bidirectional Agent2Agent (JSON-RPC/HTTP) interop wire — a discoverable A2A agent (server) and an A2A client that drives external agents, all from the one command registry
- [[Desktop-Architecture]] — how aoided, shellbridge, Quickshell, and the compositor compose into a single agent-ready desktop body
- [[Content-Pipeline]] — the discover → propose → approve → ingest → lint → query pipeline; the approve gate, quarantine branch, and Mneme integration
- [[Governance]] — the rebuild gate (polkit pattern), the single audit log, and the mutation policy encoded in radial distance from the nucleus
- [[Rebuild-Gate]] — how agents rebuild: the default propose-then-human-`switch` path, and the opt-in `aoide.rebuild` passwordless-narrow polkit capability
- [[Wiki-Protocol]] — the shipped protocol (Mneme/Melete-owned) that gives each project a standalone wiki in a shared shape; Aoide's own wiki is the self-managed exception; default location is the project's repo
- [[Package-Layout]] — target blueprint (not yet built): splitting `pkgs/aoide` into pi-style single-charter crates (protocol, conduct, server, client, storage, steward, song, management, evals, conductor, cli), the per-crate charter, the steward's canon + self-audit design, and the phased migration

## Entities

- [[aoide-cli]] — the `aoide` binary: the CLI trunk (38-command tree incl. `cover set`, `rice mint`, the `graph` group, `conduct`, and `conductor`), `schema --json` as single source of truth, the stdio MCP façade, structured exit codes, and the `aoided` daemon binary
- [[drachma]] — the design-token layer *and* the mint that stamps it: the immutable seam between nix structure and runtime rendering (schema tiers, the two-fan-out model), plus the Node package (wrapping Style Dictionary) that lints/resolves/emits the tokens — stage/drachma.json, hyprctl, and terminal OSC
- [[aoided]] — the orchestrator daemon: neutral event stream, policy, lint, audit log, and the gated rebuild pipeline
- [[shellbridge]] — the daemon-to-desktop bridge: atomic JSON state files out, unix-socket commands in, Hyprland IPC consumed
- [[Quickshell]] — the QML shell runtime (nine surfaces declared, eight with a live QML body): bar, notification daemon, gadget dock, launcher, OSD, lockscreen, greeter, wallpaper layer, session-graph (declared, no QML body today)
- [[Hyprland]] — the Wayland compositor; Aoide's only multiplexer, driven live via hyprctl
- [[Stylix]] — base16 whole-system theming; the baked fan-out from `rice.nix` to every nix-manageable target
- [[dxflake]] — the dendritic auto-discovery flake that is Aoide's prior art and adoption target for the nucleus + dendrite walker
- [[Melete]] — the integrated (not vendored) coding harness (the "doer"): autonomous coding runs, shell, GitHub, fleet, scheduling — an independent agent AoideOS launches and can be driven by; the engine behind AoideOS's widget-making
- [[Mneme]] — the integrated (not vendored) knowledge server (the "door"): the vault's MCP API behind the content pipeline and the wiki protocol

## Sources

Primary source for this wiki: [[references/AOIDE-HANDOFF]].
