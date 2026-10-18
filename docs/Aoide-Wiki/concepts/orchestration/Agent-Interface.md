---
type: concept
created: 2026-07-25
updated: 2026-08-27
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
The CLI is the complete surface; the other two are generated from the commands
that declare them, and a command is closed to MCP and A2A until it declares
`mcp` or `a2a` in its `doors`. The crate layout ([[Package-Layout]]) carries this
invariant into a structural boundary: the `aoide-protocol` crate holds the
registry, schema, and door types, so every door depends on the one contract
rather than converging on it by convention.

MCP is a façade over the declared set: its tool list is the commands whose
`doors` name `mcp`, read from the same command schema that backs the CLI
(`aoide schema --json` shows each command's `doors`).
The MCP layer is deliberately optional and agent-added:

| Setting | Value |
|---|---|
| Default | `mcp.enable = false` |
| Per-session | agents spawn `aoide mcp serve --stdio` |
| Network MCP | user-only; never agent-enabled |

Over MCP the session read commands (roster, `session find`, `graph`) answer
only for the caller's own attested subtree: its session and every session under
it by attested spawn edges. An unattested caller sees nothing. `mail` read is
not opened to MCP, so the caller cannot read a mailbox it does not own.

An MCP caller is attested the way a CLI caller is, and never approves. The stdio
server dispatches in its own process, so the gate walks that process's ancestry
to the sealed session it runs under. A send from a server launched under a
conducted session earns that session's parent and sibling rules and delivers: an
orchestrator's MCP server can talk to the children it spawned. A send from any
other server, or to a session the sealed one has no rule for, parks as pending.
`--yes` and the autogate switch are never read over MCP, and `session pending
approve` and `deny`, `session permit` and `mail ring` are refused there for
every caller, attested or not, as is anything that would rewrite a session's
parent (the parent rule reads that field, so a new session may register under a
parent but an existing one is never re-parented over MCP). Approval comes from a
terminal (the CLI) or the conductor. A `spawn` follows the same line: from an
attested caller it launches the child under that session and its first prompt
delivers, and from any other caller the whole spawn parks until a human approves
it. A `resurrect` is judged the same way, since it launches processes: an
attested caller revives the sessions it names, and from any other caller
`resurrect --project …` parks whole until a human approves it (bare `resurrect`,
which reads the working directory's manifest, is refused there and taught to
name a project). The restore that resurrect would type into a revived terminal is not a send a
caller makes, so it stays held over MCP either way. The A2A door has no sealed
ancestor, so it parks every send. See [[Conductor-Channel|the gate]] for the
doors and the rule.

## A2A: the interop door

The [[A2A-Door|A2A door]] (Agent2Agent, a Linux Foundation protocol) is the
third door — the standard wire by which aoide interoperates with *other*
agents over JSON-RPC-2.0/HTTP. It is **bidirectional**: aoide is both a
discoverable A2A *agent* (`aoide a2a serve`, off by default, loopback-bound)
whose AgentCard is generated from the same registry as the MCP tool list, and
an A2A *client* — the `node` group (`node add`/`node spawn`/`send
--to`) — that registers and drives external aoide instances, folding each
into the [[Session-Graph]]. Its capability is admitted
at rebuild time rather than per request; see [[A2A-Door]] for the door, the
concept mapping, and its security model.

## Guidance tiers

This page holds the four-tier onboarding ladder; the repo root `AGENTS.md`
points here, and `aoide guide` prints the same map at runtime. `aoide
onboard` ([[Clone-and-Run]]) ends by printing this same guide, so a fresh
install lands the agent at tier 0. An agent orients through the tiers in
order:

| Tier | Surface | Scope |
|---|---|---|
| 0 | `aoide guide` + repo root `AGENTS.md` + `docs/agent/` | Onboarding: the core-vs-paint boundary, the house rules, the read order. Read before acting; the house rules are non-negotiable. |
| 1 | the CLI — `aoide <cmd>` / `strx <cmd>` | Full capability. `aoide` is the complete orchestration surface, `strillix` the complete painted surface (AoideOS); the registry behind `schema --json` enumerates every command of each. Both carry the same house rules. |
| 2 | stdio MCP | Per-session, agent-spawned, optional — `aoide mcp serve --stdio`. Off by default. Attested by the conducted session it runs under; the harness's own MCP permission prompt is the user's approval for one connection, and aoide keeps no activation state. |
| 3 | network MCP | User-enabled only, never agent-enabled: the dedicated Aoide connector (below). Has no sealed ancestor, so every send and spawn through it parks. |

`aoide schema --json` / `strx schema --json` is the machine-readable
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

Its server runs under systemd, not under a conducted session, so attestation
finds no sealed session behind it and everything it sends or spawns parks for
a local human to approve. There is no separate network door and no network
exception to the gate. One caveat follows from what attestation sees: it names
who launched an MCP server, not who writes to its stdin. A network bridge that
someone runs inside a conducted terminal lends that terminal's rights to its
remote callers, so run any bridge outside a conducted session.

## Agent-first ergonomics

Every `aoide` command is designed as an API that happens to be typeable:

- `--json` flag on every command for structured input and output
- Structured errors with meaningful exit codes
- Published schemas for all state files (`stage/`, theme, manifests)
- All operations idempotent; output reports exactly what changed

## The hooked agents: claude and kimi

Two harnesses are first-class agent paths through the hook door, dispatched
through per-harness profiles (`aoide_protocol::agents`, see
[[Agent-Hooking]]). A
spawn wrapper registers the agent session and window address with
[[shellbridge]]; the harness's hooks post state after each operation — claude
via `Notification` / `Stop` / `Pre-PostToolUse`, kimi via the same core events
plus its dedicated `PermissionRequest`. `aoide session hook --agent
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
