---
type: concept
created: 2026-07-25
updated: 2026-08-25
tags: [aoide, agent, cli]
source: "[[references/AOIDE-HANDOFF]]"
---

# Agent Interface — CLI Trunk, MCP Façade, A2A Door

## CLI as the capability surface

`aoide <cmd>` is the complete orchestration surface: any agent with a shell
is fully capable, no MCP required (the house rule stated in full in the repo
root `AGENTS.md`). The `melete aoide …` passthrough form works identically,
so Melete's own tooling routes through the same trunk.

## One implementation, three doors

Three doors open onto aoide — the CLI trunk, the MCP façade, and the
[[A2A-Door|A2A door]] — and all three derive from the one command schema
(`aoide schema --json`). There is one implementation; the doors cannot drift.
The CLI is the complete surface; the other two are generated from it and
**off by default**. The crate layout ([[Package-Layout]]) carries this
invariant into a structural boundary: the `aoide-protocol` crate holds the
registry, schema, and door types, so every door depends on the one contract
rather than converging on it by convention.

MCP is generated as a façade from the same command schema that backs the CLI.
The MCP layer is deliberately optional and agent-added:

| Setting | Value |
|---|---|
| Default | `mcp.enable = false` |
| Per-session | agents spawn `aoide mcp serve --stdio` |
| Network MCP | user-only; never agent-enabled |

## A2A: the interop door

The [[A2A-Door|A2A door]] (Agent2Agent, a Linux Foundation protocol) is the
third door — the standard wire by which aoide interoperates with *other*
agents over JSON-RPC-2.0/HTTP. It is **bidirectional**: aoide is both a
discoverable A2A *agent* (`aoide a2a serve`, off by default, loopback-bound)
whose AgentCard is generated from the same registry as the MCP tool list, and
an A2A *client* (`aoide a2a agent add|send`) that registers and drives external
A2A agents, folding each into the [[Session-Graph]]. Its capability is admitted
at rebuild time rather than per request; see [[A2A-Door]] for the door, the
concept mapping, and its security model.

## Guidance tiers

This page holds the four-tier onboarding ladder; the repo root `AGENTS.md`
points here, and `aoide guide` prints the same map at runtime. An agent
orients through the tiers in order:

| Tier | Surface | Scope |
|---|---|---|
| 0 | `aoide guide` + repo root `AGENTS.md` + `docs/agent/` | Onboarding: the core-vs-paint boundary, the house rules, the read order. Read before acting; the house rules are non-negotiable. |
| 1 | the CLI — `aoide <cmd>` / `lyra <cmd>` | Full capability. `aoide` is the complete orchestration surface, `lyra` the complete painted surface (AoideOS); the registry behind `schema --json` enumerates every command of each. Both carry the same house rules. |
| 2 | stdio MCP | Per-session, agent-spawned, optional — `aoide mcp serve --stdio`. Off by default. |
| 3 | network MCP | User-enabled only, never agent-enabled: the dedicated Aoide connector (below). |

`aoide schema --json` / `lyra schema --json` is the machine-readable
backstop at any tier; the MCP tool list generates from it. Tier 0's
checkout half is `docs/agent/README.md` (the read-order router) and
`docs/agent/session.md` (the mechanical session checklist); the house
rules and docs layering live in the root `AGENTS.md` itself.

### Tier 3: the Aoide connector

Tier 3 is the **Aoide MCP connector**: a dedicated user-enabled connector,
separate from the [[Mneme]] connector (the vault door) and the [[Melete]]
connector (the doer harness), scoped specifically to managing Aoide and its
components — the rice loop, widget-maker, content pipeline, and daemon
status. The same one-schema-two-doors rule applies: it is the same generated
MCP façade as the stdio path, just network-exposed by the user (Tailscale
tailnet or Cloudflare funnel). It is never agent-enabled; the user enables it
once and it stays up.

## Agent-first ergonomics

Every `aoide` command is designed as an API that happens to be typeable:

- `--json` flag on every command for structured input and output
- Structured errors with meaningful exit codes
- Published schemas for all state files (`stage/`, livery, manifests)
- All operations idempotent; output reports exactly what changed

## The hooked agents: claude and kimi

Two harnesses are first-class agent paths through the hook door, dispatched
through per-harness profiles (`aoide_protocol::agents`, see
[[Agent-Hooking]]). A
spawn wrapper registers the agent session and window address with
[[shellbridge]]; the harness's hooks post state after each operation — claude
via `Notification` / `Stop` / `Pre-PostToolUse`, kimi via the same core events
plus its dedicated `PermissionRequest`. `aoide graph session hook --agent
<name>` selects the profile (default `claude`; unknown names get a structured
`unknown-agent` error listing the registered ones), and **`aoide hooks install
<agent> [--capture]`** wires the harness's settings file to pipe its hook
stream into that door — an idempotent, never-clobbering merge into
`~/.claude/settings.json` (JSON) or the kimi `config.toml` under
`$KIMI_CODE_HOME` (default `~/.kimi-code`; TOML `[[hooks]]` tables),
reporting added/present per event.
`--capture` is a temporary debugging wrap that tees raw payloads to
`~/Aoide/state/<agent>-hooks.jsonl`. Other agents receive the wrapper or fall
back to process-signal states.

## Related

- [[aoided]]
- [[shellbridge]]
- [[Clone-and-Run]]
- [[Desktop-Architecture]]
- [[Wiki-Protocol]]
- [[Session-Graph]]
- [[aoide-cli]]
- [[Agent-Hooking]]
- [[Codebase]]
- [[A2A-Door]]
- [[Package-Layout]]
