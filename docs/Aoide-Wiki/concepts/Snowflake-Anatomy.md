---
type: concept
created: 2026-07-25
tags: [aoide, architecture, nix]
source: "[[references/AOIDE-HANDOFF]]"
---

# Snowflake Anatomy — the Frozen Half

Aoide names its nix layer after snowflake morphology. The metaphor is not decorative: Nix's own logo is a snowflake, crystals grow by local accretion from a nucleus outward (exactly how dendritic self-registration works), and no two crystals are alike — same physics (shared upstream flake), unique host instances.

## The Four Layers

| Layer | Morphology term | What it is |
|---|---|---|
| Core system everything condenses around | **nucleus** | `modules/nucleus/` — aoided, shellbridge, policy, CLI |
| Branching opt-in features | **dendrites** | `modules/dendrites/` — one tree, all branches, upstream + personal |
| Render surfaces that read only notes | **facets** | `modules/facets/` — quickshell, stylix, compositor |
| Deposited aesthetic layer | **rime** | `modules/rime/` — rice engine + shipped default rices |

All four layers live under `modules/` in `~/Aoide` (the fork) — the whole snowflake is one walked module tree, keeping the repo surface to the subsystem (`modules/`) plus standard furniture, the same grouping [[dxflake]] uses. Facets are the only nix that renders appearance, and each reads only the `aoide.notes` option — no module reads another module.

## Mutation Policy

Radial distance from the nucleus encodes who may change a layer and how often:

- **Inherited structure** (nucleus, facets, rime) — change via upstream merge only.
- **New dendrite branches** — one host line to enable; growth is additive, so upstream merges stay conflict-free by construction.
- **`song/`** — the agent's writable domain, gated by User approval at the rebuild step.

This is provenance, not path fences. Personal branches (`flatpak.nix`, `firefox.nix`, `jellyfin.nix`, …) grow in the same `modules/dendrites/` tree as upstream-shipped ones. Ownership is tracked by git merge-base; `aoide update` detects divergence from inherited files and warns.

## Dendritic Walker

The composition engine is an in-house dendritic walker: every file placed under a walked directory self-registers — no explicit import list needed. [[dxflake]] is the adoption target for the nucleus + dendrite import patterns. The walker replaces the `den` tool (pattern prior art only; not a dependency). As built (commit f3ceadf) it is `lib/walk.nix` — `listFilesRecursive` filtered to `.nix` files without a `/_` infix — and `lib/mkHost.nix` walks the whole `modules/` tree into each host and injects the discovered-packages overlay from the same `callPackage` paths the flake uses (see [[Codebase]]).

**Songs self-register on the same principle.** `lib/mkHost.nix` walks `song/repertoire/` in addition to `modules/`. Each song's `rice.nix` guards itself with `lib.mkIf (config.aoide.song == "<name>")` — only the selected song activates per host. Committing a new song to the fork is sufficient to make it fleet-available; no import list needs updating. The `aoide.song` option (str, default `"default"`) selects the performance per host, set in `hosts/<host>/default.nix` as a one-line declaration. See [[Song-Vocabulary#Replay — any song, any host]] and [[Self-Ricing#Adopt, Select, Replay]].

**Packages self-register too.** `pkgs/` is walked by `lib/pkgs.nix` (sibling to the module walker, same `_`-prefix shelving): dropping `pkgs/<name>/default.nix` registers it into the flake `packages` output, the host + vm overlays, and an auto-generated `pkg-<name>` check — from one source, no hand-list to edit (see [[Codebase]]).

## Repo Layout (high level)

```
~/Aoide/
├── modules/        the snowflake — four layers, walker-discovered
│   ├── nucleus/    core daemon, CLI, policy
│   ├── dendrites/  all branches: shipped + personal
│   ├── facets/     quickshell · stylix · compositor
│   └── rime/       rice engine + default rices
├── hosts/
│   ├── common/     cross-machine baseline
│   └── <host>/     machine-specific picks
├── pkgs/           aoide CLI · note package
├── lib/            dendritic walker + helpers (checks ride as a flake output)
├── docs/
└── song/           the performed half (see [[Song-Vocabulary]])
```

The repo surface is only the subsystem (`modules/`) plus standard flake furniture (`hosts/`, `pkgs/`, `lib/`, `docs/`) and the `song/` content tree — no scaffolding sprayed across the root. Runtime dirs (`song/{stage,backstage,auditions}`, root `log/`, `index/`, `catalog/`) are gitignored and created at runtime, never committed. `hosts/` knows dendrites; dendrites never know hosts — the same separation [[dxflake]] enforces. Subfolders inside `modules/dendrites/` are grouping only; the walker registers every file regardless.

**The root is closed.** The directory list above is the whole surface — never invent a new top-level dir. New content lands inside the existing tree at its designated place: covers → `song/covers/`, chimes → `song/chimes/`, per-song assets → `song/repertoire/<song>/`, module assets next to their module. The lookup for content paths is the Song Map ([[Song-Vocabulary#The Song Map]]); creating a new root directory is a contract change (`CONTRACTS.md` §2), not a convenience.

## Related

- [[dxflake]]
- [[Fork-and-Run]]
- [[Notes]]
- [[Self-Ricing]]
- [[Codebase]]
