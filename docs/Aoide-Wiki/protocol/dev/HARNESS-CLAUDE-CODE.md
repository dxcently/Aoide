---
type: reference
created: 2026-08-30
updated: 2026-08-30
tags: [aoide, harness, agent, hooks, claude-code, protocol]
---

# Harness — Claude Code

One harness's specifics. The protocol itself (`DEV.md`,
`ORCHESTRATION.md`, `VERIFICATION.md`) assumes none of this; a second
harness gets its own page beside this one rather than edits to those.

Scope: how this harness supplies the tier roles and the session plumbing
the protocol asks for, and the traps that cost real time here.

---

## Wiring it to Aoide

```
aoide hooks install claude
```

Points the harness's settings file at `aoide session hook --agent claude`
and symlinks the repo's skill directory into the harness's skills dir.
Idempotent; never clobbers existing config or an unrelated file at the link
path.

Nine hook events reach `aoide session hook`, which maps each to a session
command through the agent's profile and never exits non-zero on a payload
problem:

`SessionStart` · `UserPromptSubmit` · `PreToolUse` · `PostToolUse` ·
`Notification` · `Stop` · `SubagentStop` · `PreCompact` · `SessionEnd`

`--capture` tees raw payloads for debugging; capture entries coexist with
plain ones and must be removed by hand when done.

**Never commit this harness's settings file.** The User maintains it.

---

## Supplying the tiers

This harness exposes several models at different reasoning depths and can
dispatch subagents with an explicit model override per dispatch — which is
exactly what `ORCHESTRATION.md`'s routing rule needs.

**The concrete model-to-tier mapping is a deployment detail and lives in
the orchestrator's memory, not in this wiki.** It changes when the
available models change; the tier roles do not.

Two harness affordances the protocol depends on:

- **A dispatch can name its model**, so ARCHITECT and EXECUTE need not be
  the same tier.
- **A dispatch is addressable after it finishes**, so REVIEW can be a
  genuinely separate instance and a stalled agent can be resumed instead of
  respawned cold.

---

## Traps

**Agents stall on backgrounded work.** A dispatched agent that starts a
long build with a monitoring tool, or backgrounds it, routinely parks
forever despite a brief saying "foreground". Brief the ban explicitly —
name the tool, and say "run it in the foreground and wait" — rather than
assuming the default.

**A stalled agent is recoverable and usually intact.** Message it with
"context intact, continue where you stopped". This has recovered every
observed stall; it is environmental, not the agent's work being wrong.
Respawning instead throws away context it still holds.

**Headless agents park on their own permission prompts.** A spawned
harness sitting "idle" is often waiting on an in-band approval prompt, and
approve-for-session is per-command-shape rather than blanket. Put the
harness's own auto-accept flag in the spawned command; when one parks, tail
its session log and answer with
`aoide send --id <id> --yes --submit -- "<choice>"`.

**A prompt delivered at spawn can race startup** — logged as delivered but
never landed. Re-sending after the session registers works.

**A subagent's final report is not shown to the User.** Relay what matters.
The orchestrator still reads the diff itself before landing; a report is
evidence, never the verdict.

---

## Session-scoped tooling

The harness's own task list is session-scoped and does not survive
compaction. The durable task list lives in the orchestrator's memory store
— see `DEV.md`, "Live state lives in memory".

Scratch files go to the harness's scratchpad directory, never to the repo:
a stray top-level path can trip the packaging-discovery check.
