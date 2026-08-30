---
type: reference
created: 2026-07-28
updated: 2026-08-30
tags: [aoide, development, agent, deprecated]
---

# Aoide Development — agent protocol (deprecated)

**Deprecated.** The dev protocol is now a self-contained set under
`protocol/dev/`, harness-agnostic and free of dependencies on the rest of
this wiki:

| page | holds |
|---|---|
| `protocol/dev/DEV.md` | entry point — preferences, prime loop, where state lives |
| `protocol/dev/ORCHESTRATION.md` | tiers, routing, dispatch gates, parallelism |
| `protocol/dev/VERIFICATION.md` | build, gates, proofs, repo discipline |
| `protocol/dev/HARNESS-CLAUDE-CODE.md` | one harness's specifics |

Start at `protocol/dev/DEV.md`.

This page carried the operating manual through 2026-08-30. Its open-flag
ledger moved to the orchestrator's memory store, where live state belongs;
its content was rewritten into the set above.
