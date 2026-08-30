---
type: reference
created: 2026-07-28
updated: 2026-08-30
tags: [aoide, development, agent, deprecated]
---

# Aoide Development — agent protocol (deprecated)

**Deprecated.** The dev protocol is now a self-contained set under
the `dev/` set, harness-agnostic and free of dependencies on the rest of
this wiki:

| page | holds |
|---|---|
| [[DEV]] | entry point — preferences, prime loop, where state lives |
| [[PRINCIPLES]] | what to build: everything-is-a-plugin, and nix's thesis as instructions |
| [[ORCHESTRATION]] | tiers, routing, dispatch gates, parallelism |
| [[CRAFT]] | repo handling, how code is written, the voice of commits and comments |
| [[VERIFICATION]] | build, gates, proofs, what never to run |
| [[HARNESS-CLAUDE-CODE]] | one harness's specifics |

Start at [[DEV]].

This page carried the operating manual through 2026-08-30. Its open-flag
ledger moved to the orchestrator's memory store, where live state belongs;
its content was rewritten into the set above.
