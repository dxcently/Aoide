---
type: concept
created: 2026-07-25
updated: 2026-08-27
tags: [aoide, onboarding, deployment]
source: "[[references/AOIDE-HANDOFF]]"
---

# Clone-and-Run — Installing Aoide

Aoide is a framework you clone and run. The upstream repo ships the shape-making machinery (engine, contracts, walker, facets, management tools) but never the shapes themselves. Your clone is your instance.

## Why Shared History

- **Shared history with upstream**: `aoide update` is a real `git merge`, not a package upgrade. Improvements flow in; your personal commits never conflict with upstream additions because growth is additive.
- **Reproducibility**: every default and generated rice is committed and versioned. A rebuild from the clone reproduces the entire riced system on any box. Runtime state (`song/stage/`, `state/`, `run/qml/`, the audit log) lives under `$AOIDE_ROOT` (default `~/.aoide`), separate from the clone, and holds ephemera only.
- **Your dendrites and songs are ordinary local commits**: `modules/dendrites/` growth is additive by construction, and `song/songbook/` is your writable domain. Committed songs are versioned score — every host that pulls the clone can perform any of them. One line in `hosts/<host>/default.nix` (`aoide.song = "<name>";`) selects which song a host performs. See [[Song-Vocabulary#Replay — any song, any host]].
- **A remote fork is optional**: add your own remote only to back up your instance, sync songs across your machines, or open contributions upstream. The instance itself needs nothing but the clone.
- **Module import stays possible** but secondary; the clone is the primary deployment model.

## Install in Two Steps

```
git clone <upstream> ~/Aoide
cd ~/Aoide && aoide onboard
```

That is the complete install. `onboard` is a CLI-door command and runs from inside the checkout: it performs the core half itself, delegates the nix half to `lyra onboard` when the `lyra` binary resolves, and finishes by printing the four-tier agent guide ([[Agent-Interface]]).

## First-Boot Onboarding Flow

`aoide onboard` is idempotent. The core half:

1. Register the clone as a graph project ([[Session-Graph]]).
2. Link `~/song` → `<checkout>/song`, never clobbering an existing file or symlink.
3. Seed `song/songbook/preferences.md` when absent ([[Song-Anatomy]]).
4. Ask which agent harnesses to wire — interactively, or via `--harness a,b` / `--yes` non-interactively — and run `hooks install` for each chosen harness ([[Agent-Hooking]]).

If the `lyra` binary resolves, `onboard` then delegates the nix half to `lyra onboard`:

1. Generate `./aoide.nix` (or `--out <path>`): every `aoide.*` module option — 142 today, derived live from the modules through the flake's `aoideOptions` output, never a hand-list — with its default commented out and a one-line description, plus an appendix listing the env knobs (`AOIDE_CONDUCT_AUTOGATE`, `AOIDE_TERMINAL`, `AOIDE_CORE_BIN`/`AOIDE_RICE_BIN`, `AOIDE_DISCOVERY_ADVERTISE`) as comments.
2. Print the `imports = [ ./aoide.nix ];` line for the user's own flake. Onboard never edits the flake; adding the import is the user's one manual step.

Re-running `lyra onboard` warns and backs the old file up to `<out>.bak`; a file it did not generate is refused, never overwritten.

**Done-state check**: bar shows agent session + connection state; a notification round-trips agent → center; `aoide schema --json` validates.

## Self-Update

**Status:** `aoide update` is a schema-real, exit-64 stub; arg-parsing and the audit trail exist, the merge/detection logic does not yet run.

`aoide update` fetches upstream, merges framework paths, runs the flake's `checks`, then proposes the gated rebuild — the clone updates itself, but the gate still decides. No background updaters, by house policy.

Merge hygiene (the merge-base divergence lint and commit-hook warning) and contract-bump detection are specified under [[Governance]]; contract-breaking changes are versioned in `CONTRACTS.md` and routed through the update playbook before the rebuild can discover them.

## Related

- [[Snowflake-Anatomy]]
- [[Governance]]
- [[Self-Ricing]]
- [[Agent-Interface]]
