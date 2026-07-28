---
type: concept
created: 2026-07-25
updated: 2026-07-28
tags: [aoide, agent, cli]
source: "[[references/AOIDE-HANDOFF]]"
---

# Agent Interface — CLI Trunk, MCP Façade

## CLI as the capability surface

`aoide <cmd>` is the complete capability surface. Any agent that has a shell is fully capable — no MCP required. The `melete aoide …` passthrough form works identically, so Melete's own tooling routes through the same trunk.

## MCP: one implementation, two doors

MCP is generated as a façade from the same command schema that backs the CLI. There is one implementation; the two doors cannot drift. The MCP layer is deliberately optional and agent-added:

| Setting | Value |
|---|---|
| Default | `mcp.enable = false` |
| Per-session | agents spawn `aoide mcp serve --stdio` |
| Network MCP | user-only; never agent-enabled |

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
- Published schemas for all state files (`stage/`, notes, manifests)
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
