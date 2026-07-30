---
type: index
---

# Aoide — Index

## Concepts

- [[Full-Architecture]] — answers: how do all the subsystems wire together, what are the inputs/outputs between them, where does each live in the tree (the whole-body map), and where the system stands (running live on yomi-strix since 2026-07-26)
- [[Codebase]] — answers: how does the built repo actually compose (flake outputs, the `lib/` walker + host-assembly overlay, `lib/checks.nix` + the `vm-boot` QEMU check and its test-masking lesson), what is the option contract, what does the yomi-strix profile pin and how it switched live (detached systemd-run switches, dxflake generation as rollback), what is the systemd user-unit map incl. the nucleus packages/nix modules, the socket + stage-file contracts (incl. stage-dir resolution), how do you build & verify, and what is real vs a structured not-implemented stub
- [[Widget-Maker]] — answers: why Aoide is fundamentally an extensible, declarative widget maker, how the coding agent (Melete) writes new integrations as nix+QML+adapter, and how the make-a-widget loop mirrors self-ricing
- [[Feature-Set]] — answers: what ships in the box (Melete + Mneme integrated and launched, not vendored), and the exemplar features — notification→messaging bridge, Tailscale/Cloudflare fleet management, scheduled jobs & timers widget — plus how each hooks into the system
- [[Terminal-Commander]] — answers: how the agent-session terminal widget (herdr-class) watches terminals running agents spawned by Melete or others, tracks which window belongs to which agent (shellbridge registration + hooks), and jumps by click or keybind
- [[Session-Graph]] — answers: how the project/session DAG works (nodes, anchors + spawned edges, longest-cwd-prefix anchoring, live-state merge), what the eight `aoide graph` commands do (view/emit/project/link/liveness-checked focus/prune), what the projects.json/graph.json v0 contracts hold, and how the DAG renders on the desktop (the dock's DAG gadget as primary affordance + the dormant overlay, via the shared GraphModel)
- [[Gadget-Dock]] — answers: what the agentWidgets surface became (the Win7-sidebar-homage gadgets, now a left-edge pinnable popup: hot-edge hover, SUPER+G open-and-pin, 400 ms grace auto-hide, hidden/peeking/pinned states), what the four gadgets show (terminal roster, compact DAG, clock, CPU/RAM meters), how GadgetFrame + notes-only colour carry the ASCII-Aero aesthetic, and which seams stay open (socket prune verb, telemetry stage file, layer-shell typing)
- [[Lexicon]] — answers: the whole vocabulary in one place — the three original Muses (Aoide · Melete · Mneme), the frozen (snowflake) / performed (song) split, the machinery words (door, gate, drachma, baton), why each word family was chosen, and the one loop that ties them together
- [[Snowflake-Anatomy]] — answers: what are the structural layers of the Aoide flake (nucleus / dendrites / facets / rime) and how does the walker register modules automatically
- [[Fork-and-Run]] — answers: how do you install Aoide (fork, clone, `aoide onboard`) and how does the shared-history model enable upstream updates and user contributions
- [[Self-Ricing]] — answers: what is the rice lifecycle (gen → lint → preview → adopt), what is the songbook write-back loop, how are rices versioned, how does `aoide.song` select a performance per host, and what is the transpose vs replay distinction
- [[Song-Vocabulary]] — answers: what does each "song" term map to in the performed half (key, melody, arrangement, instruments, liner, songbook, cover, chimes, stage, rehearsal, recording, venue, replay) and how replay makes a committed song host-agnostic
- [[Song-Anatomy]] — answers: what every `song/` subfolder is for (repertoire/<name> · songbook · covers · keys · chimes · stage · backstage · auditions), which are committed score vs gitignored runtime, who writes each (song agent vs drachma/shellbridge), and what the six stage files carry
- [[Agent-Interface]] — answers: what commands does the `aoide` CLI expose, how does MCP relate to the CLI, what are the guide tiers, and what ergonomics apply to all commands
- [[Desktop-Architecture]] — answers: how do aoided, shellbridge, Quickshell, and the compositor compose into the live desktop and what is each component's responsibility
- [[Content-Pipeline]] — answers: what are the pipeline stages (discover → propose → approve → ingest → lint → query), what is the approve gate, and how does Mneme integrate
- [[Governance]] — answers: who may trigger a rebuild, where the audit log lives, and how the mutation policy (radial distance from nucleus) governs change frequency
- [[Rebuild-Gate]] — answers: how an agent turns a proposed change into a running system, what the default propose-then-human-`switch` path is, what the opt-in `aoide.rebuild` capability grants, and why a passwordless narrowly-scoped polkit unit beats giving the agent a sudo password
- [[Wiki-Protocol]] — answers: how the wiki protocol gives each project a standalone wiki in one shared shape, what its mint and convert operations do, who owns it (Mneme/Melete), and the default location (the project's repo)
- [[Ricing-Protocol|Ricing Protocol]] — answers: the creation/application split (deriving a base16 vs Stylix fanning it out), the mandatory light/dark vision-check, and where per-song design memory belongs (the songbook), worked through the `sonata` light key
- [[Conductor-Channel|Conductor Channel]] — answers: how a wrapped agent session is commanded (PTY control socket, the gated `graph send` injection door, parent-autogate)
- [[Baton-3D-DAG|Baton 3D DAG]] — answers: the PLANNED ratatui 3D-wireframe DAG view for `aoide baton` (not yet implemented — a build plan)

## Entities

- [[aoide-cli]] — answers: what the `aoide` binary exposes (the 28-command tree incl. the `graph` group, `conduct`, and `baton`, which commands are real vs structured not-implemented stubs), how `schema --json` is the single source of truth the MCP tool list generates from, the exit-code conventions (0/1/2/64), how `graph focus` verifies window liveness and its five failure reasons, how it locates `drachma`, and the second `aoided` binary
- [[drachma]] — answers: what the drachma schema is and what its tiers are, how the two-fan-out works (stage/drachma.json for rehearsal, Stylix for recording) and why it cannot drift, what the Node drachma mint does (lint/resolve/emit subcommands, the three emitters, the atomic stage write), how it wraps Style Dictionary for the tiered resolver + null→palette fallback, and where it lives in nix (`packages.drachma`, the overlay attr)
- [[aoided]] — answers: what the orchestrator daemon does (event stream, policy, lint, audit log, gated rebuild), how per-agent adapters work, and what the security boundary on notification text is
- [[shellbridge]] — answers: how the daemon/agents communicate with the live desktop (JSON state files out, socket commands in, Hyprland IPC), how the session-jump flow works, what the five stage files carry (notes, sessions + parentSessionId, hooks, projects, graph), and how the stage dir resolves (the `AOIDE_STAGE_DIR` contract seam, now honoured)
- [[Quickshell]] — answers: what shell surfaces the QML runtime provides (nine registered, incl. the dormant session-graph overlay and the gadget-dock popup), why it replaces swaync/rofi/hyprlock/swww, how notes hot-reload at rehearsal, why GraphModel.qml is the one canonical graph model, where the deployed QML tree lands (~/Aoide/qml, an open deploy-target question), and what the NotificationServer spike covers
- [[Hyprland]] — answers: what role the compositor plays in Aoide (only multiplexer, no PTY), how its facet renders live via hyprctl, and how shellbridge consumes its IPC
- [[Stylix]] — answers: how base16-driven whole-system theming works in Aoide, how surface ownership is enforced between Stylix and Quickshell, and what GTK/Qt preview limitations apply
- [[dxflake]] — answers: how the dendritic auto-discovery walker works (listFilesRecursive, `/_` shelving, nucleus vs dendrites), how hosts are configured (flags only, no imports), and why it is Aoide's adoption target
- [[Melete]] — answers: what the integrated (not vendored) coding harness is (Rust/Rune/Python tiers, the daemon + MCP tool surface), its full feature set (coding dispatch, run control, shell/fleet, skills, backups, maintenance), and its part in Aoide as the widget-making engine
- [[Mneme]] — answers: what the integrated (not vendored) knowledge server is (the shell-less client's vault API — the "door"), why it is a separate daemon from Melete, its feature set (read/write/versions/trash/conventions/skills), and its part behind the content pipeline and wiki protocol
- [[Agent-Hooking]] — answers: how ANY agent harness registers on the baton — the three idempotent doors (Claude-Code-shaped hook payload, the `aoide graph session` verbs, the conduct wrapper) that write the `sessions`/`hooks`/`graph` stage files every widget and the baton render from

## References

- [[references/AOIDE-HANDOFF|AOIDE-HANDOFF]] — the original design contract: what Aoide *is* (the primary source for this wiki)
- [[references/AOIDE-DEV-HANDOFF|AOIDE-DEV-HANDOFF]] — the development agent's operating manual: the prime loop, build/show mechanics, git discipline, and the §7 open-flags live ledger
