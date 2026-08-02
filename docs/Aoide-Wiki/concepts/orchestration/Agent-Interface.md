---
type: concept
created: 2026-07-25
updated: 2026-07-28
tags: [aoide, agent, cli]
source: "[[references/AOIDE-HANDOFF]]"
---

# Agent Interface — CLI Trunk, MCP Façade, A2A Door

## CLI as the capability surface

`aoide <cmd>` is the complete capability surface. Any agent that has a shell is fully capable — no MCP required. The `melete aoide …` passthrough form works identically, so Melete's own tooling routes through the same trunk.

## One implementation, three doors

Three doors open onto aoide — the CLI trunk, the MCP façade, and the
[[A2A-Door|A2A door]] — and all three derive from the one command schema
(`aoide schema --json`). There is one implementation; the doors cannot drift.
The CLI is the complete surface; the other two are generated from it and
**off by default**.

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

Agents orient through four tiers, in order:

0. `aoide guide` (and `AGENTS.md` at a well-known path) — tier-0 onboarding
1. CLI — full capability
2. stdio MCP — structured tool layer, per-session
3. Network MCP (tailnet/funnel) — user-enabled only

`aoide schema --json` is the machine-readable backstop at any tier; the MCP tool list generates from it.

### Tier 3: the Aoide connector

Tier 3 is the **Aoide MCP connector** — a dedicated user-enabled connector, separate from the [[Mneme]] connector (the vault door) and the [[Melete]] connector (the doer harness), scoped specifically to managing Aoide and its components: the rice loop, widget-maker, content pipeline, and daemon status. The same one-schema-two-doors rule applies: it is the same generated MCP façade as the stdio path, just network-exposed by the user (Tailscale tailnet or Cloudflare funnel). It is never agent-enabled; the user enables it once and it stays up.

## Agent-first ergonomics

Every `aoide` command is designed as an API that happens to be typeable:

- `--json` flag on every command for structured input and output
- Structured errors with meaningful exit codes
- Published schemas for all state files (`stage/`, drachma, manifests)
- All operations idempotent; output reports exactly what changed

## Primary agent: claude CLI

The claude CLI is the first-class agent path. A spawn wrapper registers the agent session and window address with [[shellbridge]]; Claude Code hooks (`Notification` / `Stop` / `Pre-PostToolUse`) post state after each operation. Other agents receive the wrapper or fall back to process-signal states.

## Related

- [[aoided]]
- [[shellbridge]]
- [[Fork-and-Run]]
- [[Desktop-Architecture]]
- [[Wiki-Protocol]]
- [[Session-Graph]]
- [[aoide-cli]]
- [[Codebase]]
- [[A2A-Door]]
