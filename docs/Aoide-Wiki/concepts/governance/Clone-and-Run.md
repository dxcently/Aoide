---
type: concept
created: 2026-07-25
updated: 2026-09-27
tags: [aoide, onboarding, deployment]
source: "[[references/AOIDE-HANDOFF]]"
---

# Clone-and-Run — Installing Aoide

Aoide is a framework you clone and run. The upstream repo ships the shape-making machinery (engine, contracts, catalogue, paint lanes, management tools) but never the shapes themselves. Your clone is your instance.

## Why Shared History

- **Shared history with upstream**: `aoide update` is a real `git merge`, not a package upgrade. Improvements flow in; your personal commits never conflict with upstream additions because growth is additive.
- **Reproducibility**: every default and generated rice is committed and versioned. A rebuild from the clone reproduces the entire riced system on any box. Runtime state (`/stage/`, `state/`, `run/qml/`, the audit log) lives under `$AOIDE_ROOT` (default `~/.aoide`), separate from the clone, and holds ephemera only.
- **Your dendrites and rices are ordinary local commits**: `modules/dendrites/` growth is additive by construction, and `rices/` is your writable domain. Committed rices are versioned score — every host that pulls the clone can perform any of them. One line in `hosts/<host>/default.nix` (`habit.rice.declared = "<name>";`) selects which rice a host performs. See [[Rice-Vocabulary#Replay — any rice, any host]].
- **A remote fork is optional**: add your own remote only to back up your instance, sync rices across your machines, or open contributions upstream. The instance itself needs nothing but the clone.
- **Module import stays possible** but secondary; the clone is the primary deployment model.

## Install in Two Steps

```
git clone <upstream-url> ~/Aoide       # `-b main` when the source is a bundle
cd ~/Aoide && aoide onboard
```

Those two lines install the **distribution** — the clone, the host line, the
rices. They do not install `aoide` itself: the binary comes from the
checkout with cargo, no nix, and that step has one page of its own,
`docs/INSTALL.md` (prerequisites, the clone, `cargo install --path
pkgs/aoide/crates/cli --bins --locked`, first boot, the smoke test, starting
`aoided`). Read it first; nothing below presumes a binary the page never
installs.

With `aoide` on `PATH`, the two lines above are the whole thing. `onboard` is
a CLI-door command and runs from inside a checkout: it performs the core half
(the clone registration and the harness hooks) and finishes by printing the
four-tier agent guide ([[Agent-Interface]]). The desktop half is `strx onboard`,
run after it: the rice seed, the `~/rice` link and the nix half. Core never runs
it, so a box with no nix and no `strillix` completes the core half and says so.

## First-Boot Onboarding Flow

`aoide onboard` is idempotent and is the core half:

1. Register the clone as a graph project ([[Session-Graph]]).
2. Ask which agent harnesses to wire — interactively, or via `--harness a,b` / `--yes` non-interactively — and run `hooks install` for each chosen harness ([[Agent-Hooking]]).

It closes by printing the agent guide and this machine's node line.

The desktop half is `strx onboard`, run after it:

1. Generate `./aoide.nix` (or `--out <path>`): every `aoide.*` module option — 161 today, derived live from the modules through the flake's `aoideOptions` output, never a hand-list — with its default commented out and a one-line description, plus an appendix listing the env knobs (`AOIDE_CONDUCT_AUTOGATE`, `AOIDE_TERMINAL`, `AOIDE_CORE_BIN`/`STRILLIX_BIN`, `AOIDE_DIALOG_CMD`, `AOIDE_DISCOVERY_ADVERTISE`) as comments.
2. Print the `imports = [ ./aoide.nix ];` line for the user's own flake. Onboard never edits the flake; adding the import is the user's one manual step.
3. From a checkout: seed `rices/preferences.md` when absent ([[Rice-Anatomy]]), and link `~/rice` → `<checkout>/rice`, never clobbering an existing file or symlink.

Re-running `strx onboard` warns and backs the old file up to `<out>.bak`; a file it did not generate is refused, never overwritten.

: bar shows agent session + connection state; a notification round-trips agent → center; `aoide schema --json` validates.

On a NixOS host the daemon is started for you: `pkgs/aoide/module/aoided.nix` writes
the `aoided` systemd **user** unit and anchors it to `aoide.sessionTarget`. That module is the declared path. Off nix — a plain Linux
box, a container, WSL2 — there is no unit file, and `docs/INSTALL.md` § 6 carries
the portable equivalent (the unit body, its four environment seams, and the
no-systemd fallback).

## Self-Update

**Status:** `aoide update` is a schema-real, exit-64 stub; arg-parsing and the audit trail exist, the merge/detection logic does not yet run.

`aoide update` fetches upstream, merges framework paths, runs the flake's `checks`, then proposes the gated rebuild — the clone updates itself, but the gate still decides. No background updaters, by house policy.

Merge hygiene (the merge-base divergence lint and commit-hook warning) and contract-bump detection are specified under [[Governance]]; contract-breaking changes are versioned in `CONTRACTS.md` and routed through the update playbook before the rebuild can discover them.

## Related

- [[Snowflake-Anatomy]]
- [[Governance]]
- [[Self-Ricing]]
- [[Agent-Interface]]
