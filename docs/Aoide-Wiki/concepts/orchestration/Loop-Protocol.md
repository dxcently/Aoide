---
type: concept
created: 2026-08-20
updated: 2026-08-27
tags: [aoide, agent, orchestration, harness, session]
---

# Loop Protocol — the harness-agnostic agent loop

## Framing

The loop protocol is a specification of role, gate, and degradation behavior
laid over primitives Aoide already has — `spawn`, `send`, `session
pending list|approve|deny`, bare `graph`, a session's log, a session's
transcript — not a command of its own. Every consequential step a multi-role
run takes is a judgment call, exercised by an agent reading the brief in
front of it: ship a change versus send it back, coach an executor
mid-flight, kill a stuck agent. The protocol leaves that judgment exactly
where it is
exercised, and hands it only the primitives it acts through: `spawn`
opens a fresh-context unit, `send` and the pending queue steer or hold
one, `graph` and a unit's log or transcript observe one. Roles, gates,
and the send-back cycle are documented behavior over those primitives, kept
in one place, read before a run rather than hard-coded ahead of one.

## The tier, mechanism-agnostic

A **tier** is one fresh-context execution unit per role — planner, executor,
reviewer each run in a context that holds nothing from the other two, so no
role's judgment carries contamination from another role's working state. The
mechanism that opens that fresh context is not part of the definition: a
harness's own internal subagent tool (claude's `Task`/`Agent`, kimi's
`Agent`) and a `spawn` session are two BINDINGS of the same tier
concept. Both hand a role a fresh context, steerable and observable through
aoide's commands; neither binding outranks the other. Which binding a given role
uses is a per-role decision (see Binding rule), not a property of the tier
itself.

## Two-rung ladder

- **R1 TIERED (default).** Each role — planner, executor, reviewer — runs in
  its own fresh-context unit, bound to an internal subagent or a `spawn`
  session per the binding rule.
- **R2 SINGLE-AGENT (degraded).** One context plans, executes, and reviews
  its own work. R2 applies only when the orchestrator's running harness
  offers no internal subagent tool AND `spawn` is unavailable to it.

`spawn` runs against any conducted harness, so R1 is universally
reachable. "Tiered is the default" is a rule: run R1 unless the harness in
front of you genuinely offers neither binding, not a per-harness declaration
requiring a lookup table.

## Binding rule

Which binding a role uses is judgment per brief, weighed against one piece of
guidance: a long-running executor binds to a `spawn` session —
observable, steerable, and it survives the orchestrator losing its own
context; short, scoped work binds to an internal subagent. Three conditions
force `spawn` regardless of how short the work looks:

- the role crosses a harness or model boundary the orchestrator itself isn't
  running on,
- the brief needs mid-flight steering,
- the user needs to observe the role directly — a `spawn` session
  carries a log, a graph node, and a herald; an internal subagent carries
  none of those outward.

Model-tiering — routing a role to a particular model, planner on one,
executor on another — is orthogonal seasoning applied on top of either
binding, not a substitute for fresh-context tiering.

## Review integrity

The reviewer is never the executor's own context, on any rung, in any
harness — a review sharing context with what it reviews is not a review. A
distinct model or harness fills the reviewer seat whenever the running
harness offers one. The reviewer's brief always carries the grading
discipline: grade IS THIS WRONG, not DID MY CHANGE REGRESS IT. That
discipline travels with the brief rather than the binding — a same-model,
same-context rationalization is a briefing failure, and swapping in a
distinct model or harness for the reviewer seat does not fix a brief that
omits it.

## Degradation announcement

A run on R2 announces its own rung in its landing log — the record a run
leaves states which rung it ran on, plainly, alongside the work. Self-review
is not review, and an R2 landing log carries that fact rather than reading
like a passed one.

## Harness status

**claude** binds R1 through its internal subagent tool (`Task`/`Agent`) or a
`spawn` session, judgment per the binding rule.
**kimi** binds R1 through its `Agent` tool or a `spawn` session,
judgment per the binding rule.
**pi holds no rung on the ladder** — see [[Conductor-Channel]]'s headless
section.

## Related

- [[Conductor-Channel]]
- [[Session-Graph]]
- [[Agent-Hooking]]
- [[Terminal-Commander]]
