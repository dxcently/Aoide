---
type: index
updated: 2026-08-26
---

# Aoide — Index

## Concepts

- [[Full-Architecture]] — the whole-body map: how the subsystems wire together, their inputs/outputs, where each lives in the tree, and system status
- [[Plugin-Architecture]] — the design philosophy behind every contract: capabilities enter by existing at a conventional path; bridges first, QML second
- [[Codebase]] — repo composition: flake outputs, `lib/` walker + overlay, option contract, systemd map, socket/stage contracts, build-and-verify, real vs stubbed
- [[Widget-Maker]] — why Aoide is a declarative widget maker: the coding agent writes new integrations as nix + QML + adapter; the make-a-widget loop
- [[Widget-Bridge-Contract]] — widgets as pure views of the bridge: the `sessions.json` field contract, the hook→state machine, and the rules a widget obeys
- [[Feature-Set]] — what ships in the box (Melete + Mneme integrated) and the exemplar features: messaging bridge, fleet management, scheduled-jobs widget
- [[Terminal-Commander]] — the agent-session terminal widget: tracks which window runs which agent, jumps by click or keybind
- [[Session-Graph]] — the project/session DAG: nodes, anchors, spawned edges, the `aoide graph` commands, the graph.json contracts, and today's render paths
- [[Gadget-Dock]] — the `AoidePanel.qml` left-edge dock: hot-edge hover, SUPER+G, four core gadgets plus an opt-in Usage stele, livery-only colour
- [[Lexicon]] — the whole vocabulary: the three Muses, the frozen/performed split, the machinery words, and why each word family was chosen
- [[Snowflake-Anatomy]] — the flake's structural layers (nucleus / dendrites / facets / rime) and how the walker registers modules automatically
- [[Clone-and-Run]] — installing Aoide: clone, `aoide onboard`; the shared-history model for upstream updates and optional contributions
- [[Self-Ricing]] — the rice lifecycle (compose → stage → lint → draft → declare), the songbook write-back, and the three-way `rice mode` gate
- [[Song-Vocabulary]] — what each song term maps to in the performed half, and how replay makes a committed song host-agnostic
- [[Song-Anatomy]] — what every `song/` subfolder is for, committed score vs gitignored runtime, who writes each, and the six stage files
- [[Agent-Interface]] — the `aoide` command surface, MCP as a generated façade, the guide tiers, and per-command ergonomics
- [[A2A-Door]] — the bidirectional Agent2Agent door: `a2a serve` agent, the `peer` group as the outbound client, concept mapping, rebuild-time admission, token auth
- [[Peer-Federation]] — folding another instance's session graph in as a subtree: `state/peers.json`, the peer-cache TTL, the `peer` CLI group; same-network only
- [[Secrets-Broker]] — the `aoide secrets` door: socket-only broker, TOTP-gated parked resolves, `secrets watch`/`--popup`, age default backend
- [[Desktop-Architecture]] — how aoided, shellbridge, Quickshell, and the compositor compose into the live desktop; each component's responsibility
- [[Content-Pipeline]] — the pipeline stages (discover → propose → approve → ingest → lint → query), the approve gate, Mneme integration
- [[Governance]] — who may trigger a rebuild, where the audit log lives, and mutation policy by radial distance from the nucleus
- [[Rebuild-Gate]] — propose-then-human-`switch` by default; the opt-in passwordless-narrow `aoide.rebuild` polkit capability and why it beats a sudo password
- [[Wiki-Protocol]] — the Mneme/Melete-owned protocol giving each project a standalone wiki in one shared shape; mint and convert
- [[Ricing-Protocol|Ricing Protocol]] — the creation/application split (deriving base16 vs Stylix fan-out), the light/dark vision-check, per-song design memory
- [[Conductor-Channel|Conductor Channel]] — commanding a wrapped agent session: PTY control socket, the gated `graph send` injection door, parent-autogate
- [[Loop-Protocol|Loop Protocol]] — harness-agnostic multi-role agent loops: per-role fresh contexts, the R1/R2 degradation ladder, review integrity
- [[Conductor-3D-DAG|Conductor 3D DAG]] — the ratatui 3D-wireframe DAG view for `aoide conductor`. Status: specified, not implemented
- [[Package-Layout]] — the landed pi-style single-charter crate split of `pkgs/aoide`, the per-crate charters, and the two-binary split (`aoide`/`aoided` vs `lyra`)
- [[Agent-Hooking]] — how any harness registers on the conductor: hook payloads through the `AgentProfile` seam, the `graph session` commands, the conduct wrapper
- [[Conductor-TUI]] — the `aoide conductor` interactive terminal frontend: seven panels, keys, what each dispatches
- [[Secrets-Commands]] — the `aoide secrets` credential door's 16 commands: direct-home admin, over-the-socket operator, and the daemon itself
- [[Controls]] — the day-to-day surface: rebuild aliases, compositor keybinds, the bar's click/scroll/hover interactions, shell aliases
- [[Screen-Control]] — `lyra screen`, AoideOS's computer-use surface: look, ground, act, verify — fourteen commands behind one CLI group

## Entities

- [[aoide-cli]] — the `aoide` binary: the self-registering command registry behind `dispatch()`/`schema --json`/MCP, exit codes 0/1/2/64, and the `aoided` daemon
- [[lyra]] — the AoideOS paint binary: its own registry, its own 43-command schema, the nix boundary core never crosses
- [[livery]] — the livery schema and tiers, the two-fan-out (stage/livery.json + Stylix), the native `lyra livery` engine and its four emitters
- [[aoided]] — the orchestrator daemon: event stream, policy, lint, audit log, gated rebuild; per-agent adapters; the notification-text security boundary
- [[shellbridge]] — daemon↔desktop: JSON state files out, socket commands in, Hyprland IPC; the stage files and the `AOIDE_STAGE_DIR` seam
- [[Quickshell]] — the QML shell surfaces (nine declared, five live), livery hot-reload, the `AoidePanel.qml` dock, and the QML deploy path
- [[Hyprland]] — the compositor's role: Aoide's only multiplexer, live hyprctl rendering, shellbridge IPC consumer
- [[Stylix]] — base16 whole-system theming; surface ownership between Stylix and Quickshell; GTK/Qt preview limitations
- [[dxflake]] — the dendritic auto-discovery walker (listFilesRecursive, `/_` shelving), flags-only hosts; Aoide's adoption target
- [[Melete]] — the integrated coding harness: daemon + MCP surface, coding dispatch, shell/fleet, skills; Aoide's widget-making engine
- [[Mneme]] — the integrated knowledge server: the vault API behind the content pipeline and the wiki protocol

## References

- [[references/AOIDE-HANDOFF|AOIDE-HANDOFF]] — the original design contract: what Aoide is (the primary source for this wiki)
- [[AOIDE-DEV|AOIDE-DEV]] — the development agent's operating manual: the prime loop, build/show mechanics, git discipline, the §7 open-flags ledger
- [[CLI-Reference]] — the per-command dev reference: signature, files read/written, output targets; hub page plus eight group pages
