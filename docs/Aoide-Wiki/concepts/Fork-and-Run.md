---
type: concept
created: 2026-07-25
updated: 2026-07-28
tags: [aoide, onboarding, deployment]
source: "[[references/AOIDE-HANDOFF]]"
---

# Fork-and-Run — Installing Aoide

Aoide is a framework you fork, not a package you install. The upstream repo ships the shape-making machinery (engine, contracts, walker, facets, management tools) but never the shapes themselves. Your fork is your instance.

## Why a Fork

- **Shared history with upstream**: `aoide update` is a real `git merge`, not a package upgrade. Improvements flow back; your personal branches never conflict with upstream additions because growth is additive.
- **Reproducibility**: every default and generated rice is committed and versioned. A rebuild from the fork reproduces the entire riced system on any box. Gitignored runtime dirs (`stage/`, `auditions/`) hold ephemera only — nothing reproduction needs lives there.
- **Songs travel with the fork**: committed songs under `song/songbook/` are versioned score — every host that pulls the fork can perform any of them. One line in `hosts/<host>/default.nix` (`aoide.song = "<name>";`) selects which song a host performs. This is how replay works across the fleet. See [[Song-Vocabulary#Replay — any song, any host]].
- **Module import stays possible** but secondary. The fork is the primary deployment model.

## Install in Two Steps

```
git clone <your-fork> ~/Aoide
aoide onboard
```

That is the complete install, by design — no dotfile manager, no separate bootstrap script. **Status:** `aoide onboard` is declared in the schema but not yet implemented (stub, exit `64`); the flow below is the target shape, not a working install path today.

## First-Boot Onboarding Flow

`aoide onboard` is designed to be idempotent, running through these steps:

1. Generate `hosts/<hostname>/` from the fork template; create `song/` runtime dirs (gitignored).
2. Install the shipped default rice + wallpaper as the active baseline; link `~/song` → `~/Aoide/song`.
3. Set the upstream remote so `aoide update` has a target.
4. Seed `songbook/` with starter files and the update playbook.
5. Write `AGENTS.md` to its well-known path; print the four-tier agent guide.
6. Detect `claude` CLI; offer the stdio MCP registration line and spawn-wrapper install. Other agents get shell instructions.
7. Offer integration toggles: Mneme source registration, Obsidian (off by default).
8. Walk an approve-gate demo: register a scratch folder, watch it flow discover → propose → approve → query.
9. Run `aoide rice preview` on the shipped default to verify the live loop.

**Done-state check**: bar shows agent session + connection state; a notification round-trips agent → center; `aoide schema --json` validates.

## Self-Update

`aoide update` (planned — stub, exit `64`) is designed to fetch upstream, merge framework paths, run the flake's `checks`, then propose the gated rebuild — the fork updates itself, but the gate still decides. No background updaters, by house policy.

Merge hygiene is enforced by a merge-base divergence lint inside `aoide update` plus a commit-hook warning on edits to inherited files. Path guards are not used; provenance is the mechanism.

Contract-breaking changes (drachma schema, dendrite shape, stage file formats) are versioned in `CONTRACTS.md`. `aoide update` detects contract bumps and routes them through the update playbook before the rebuild can discover them.

## Related

- [[Snowflake-Anatomy]]
- [[Governance]]
- [[Self-Ricing]]
- [[Agent-Interface]]
