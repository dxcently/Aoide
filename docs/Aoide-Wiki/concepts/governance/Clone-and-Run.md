---
type: concept
created: 2026-07-25
updated: 2026-08-14
tags: [aoide, onboarding, deployment]
source: "[[references/AOIDE-HANDOFF]]"
---

# Clone-and-Run — Installing Aoide

Aoide is a framework you clone and run, not a package you install. The upstream repo ships the shape-making machinery (engine, contracts, walker, facets, management tools) but never the shapes themselves. Your clone is your instance.

## Why Shared History

- **Shared history with upstream**: `aoide update` is a real `git merge`, not a package upgrade. Improvements flow in; your personal commits never conflict with upstream additions because growth is additive.
- **Reproducibility**: every default and generated rice is committed and versioned. A rebuild from the clone reproduces the entire riced system on any box. Gitignored runtime dirs (`stage/`, `auditions/`) hold ephemera only — nothing reproduction needs lives there.
- **Your dendrites and songs are ordinary local commits**: `modules/dendrites/` growth is additive by construction, and `song/songbook/` is your writable domain — a plain clone carries them with no fork required. Committed songs under `song/songbook/` are versioned score — every host that pulls the clone can perform any of them. One line in `hosts/<host>/default.nix` (`aoide.song = "<name>";`) selects which song a host performs. This is how replay works across the fleet. See [[Song-Vocabulary#Replay — any song, any host]].
- **A remote fork is optional**: add your own remote only to back up your instance, sync songs across your machines, or open contributions upstream. The instance itself needs nothing but the clone.
- **Module import stays possible** but secondary. The clone is the primary deployment model.

## Install in Two Steps

```
git clone <upstream> ~/Aoide
aoide onboard
```

That is the complete install, by design — no dotfile manager, no separate bootstrap script. **Status:** `aoide onboard` is declared in the schema but not yet implemented (stub, exit `64`); the flow below is the target shape, not a working install path today.

## First-Boot Onboarding Flow

`aoide onboard` is designed to be idempotent, running through these steps:

1. Generate `hosts/<hostname>/` from the shelved skeletons (`hosts/_desktop`, `_laptop`, or `_server` — `_mac` is the forward-looking darwin one, pending the `mkHost` class seam); create `song/` runtime dirs (gitignored).
2. Install the shipped standard rice + wallpaper as the active baseline; link `~/song` → `~/Aoide/song`.
3. Confirm the upstream remote (the clone already tracks it as `origin`); offer adding a personal remote for backup/fleet sync.
4. Seed `songbook/` with starter files and the update playbook.
5. Write `AGENTS.md` to its well-known path; print the four-tier agent guide.
6. Detect `claude` CLI; offer the stdio MCP registration line and spawn-wrapper install. Other agents get shell instructions.
7. Offer integration toggles: Mneme source registration, Obsidian (off by default).
8. Walk an approve-gate demo: register a scratch folder, watch it flow discover → propose → approve → query.
9. Run `lyra rice stage` on the shipped standard to verify the live loop.

**Done-state check**: bar shows agent session + connection state; a notification round-trips agent → center; `aoide schema --json` validates.

## Self-Update

**Status:** `aoide update` is a schema-real, exit-64 stub; arg-parsing and the audit trail exist, the merge/detection logic does not yet run.

`aoide update` fetches upstream, merges framework paths, runs the flake's `checks`, then proposes the gated rebuild — the clone updates itself, but the gate still decides. No background updaters, by house policy.

Merge hygiene is enforced by a merge-base divergence lint inside `aoide update` plus a commit-hook warning on edits to inherited files. Path guards are not used; provenance is the mechanism.

Contract-breaking changes (livery schema, dendrite shape, stage file formats) are versioned in `CONTRACTS.md`. `aoide update` detects contract bumps and routes them through the update playbook before the rebuild can discover them.

## Related

- [[Snowflake-Anatomy]]
- [[Governance]]
- [[Self-Ricing]]
- [[Agent-Interface]]
