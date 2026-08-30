---
type: reference
created: 2026-08-30
updated: 2026-08-30
tags: [aoide, orchestration, agent, harness, protocol]
---

# Orchestration Protocol

How a dev agent dispatches work. Harness-agnostic — roles are TIERS, and
nothing here assumes a particular agent tool. Dependency of `DEV.md`.

Aoide **is** an orchestration core. Dogfood it while building it.

---

## Three tiers

| tier | does | never does |
|---|---|---|
| **ARCHITECT** | architecture, phase plans, design records, advisement, hard trade-offs | implement |
| **EXECUTE** | implements exactly one phase against a written brief | design, or review its own work |
| **REVIEW** | re-derives the result from the diff and the repo | accept the executor's self-report |

The **orchestrator** decomposes, dispatches, verifies, lands, and logs. It
does not implement, design, or review. Small nudges stay with it: a typo, a
one-line tweak, a config bump, a read-only probe.

---

## Routing by capability

Rank the available models by reasoning depth, then:

- **Deepest available → ARCHITECT.** Design is where model quality
  compounds; a weak plan taxes every phase after it.
- **Fast tier → EXECUTE and REVIEW.** Execution against a good brief is
  throughput work.
- **Front-end and visual design** takes the strongest *design* model
  available, which is not always the deepest reasoner.
- **Only one tier available?** Run architect-shaped prompts on it with an
  explicit thinking budget, and still dispatch review as a **separate
  instance**.

**The invariant that survives any model set: the session that authored a
diff never reviews it.** A reviewer grades "is this wrong", never "did my
change regress it". Everything else on this page is preference; this is a
rule.

**Published artifacts name TIERS, never models.** Any harness-to-model
mapping is a local deployment detail. It lives in the orchestrator's
memory, not in this wiki.

---

## Dispatch gates

Per phase, in order. Each earned its place by a miss.

| gate | do |
|---|---|
| concretize | restate the ask as one phase with a definite end |
| map | read the actual code; verify every claim the brief will assert |
| advise | ARCHITECT ruling on any open fork — no forks reach EXECUTE |
| brief | scope, seams, count sites, verification, hard constraints |
| execute | one agent, foreground, nothing backgrounded |
| review | different instance, re-derives from the diff |
| land | orchestrator reads the diff itself, then commits by pathspec |
| log | commit message, wiki, memory delta |

**Briefs go stale silently.** Verify count sites, file paths, and line
numbers *at brief time*. Never copy them from memory or from a previous
brief — a brief naming a site that moved sends the executor to rewrite the
wrong thing.

**Ask every executor to report what the brief got wrong.** It is routinely
the most valuable line in the report.

**Give bounded paths and acceptance evidence.** An agent told which files
it owns cannot collide with one told the same about different files.

---

## Parallelism

- Batch independent agents so they run concurrently. Keep conclusions, not
  file dumps.
- **Serialize anything that runs cargo.** Concurrent cargo against a shared
  `target/` produces flaky counts that read as real failures.
- **Resume a stalled agent rather than respawning it cold.** A respawn
  re-derives context the stalled one still holds.
- Prefer a read-only agent when ownership of a path is unclear.

---

## Shared worktree

Other sessions edit this repository concurrently.

- `git status --short` before any dispatch.
- Never overlap writer paths between agents.
- Leave files another agent is mid-editing alone.
- Commit by explicit pathspec; never stage the whole tree.

---

## Session control

Aoide's own channel, available to any harness with a shell:

```
aoide conduct -- <cmd>                          # run as a conductable session
aoide send --id <id> [--submit] [--yes] -- <text>   # command another session
aoide session                                   # roster
aoide graph                                     # the session DAG
```

Children spawned by the orchestrator inherit its autogate; other sessions
need `--yes`. Test hook, socket, graph, and reaper changes end to end:
register, command, kill, and observe the reaping.
