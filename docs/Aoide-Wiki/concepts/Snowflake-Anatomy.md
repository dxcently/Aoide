---
type: concept
created: 2026-07-25
updated: 2026-08-25
tags: [aoide, architecture, nix]
source: "[[references/AOIDE-HANDOFF]]"
---

# Snowflake Anatomy — the Frozen Half

Aoide names its nix layer after snowflake morphology — see [[Lexicon#The frozen family — snowflake morphology]] for why.

## The Three Layers

| Layer | Morphology term | What it is |
|---|---|---|
| Core shared by every host | **nucleus** | `modules/nucleus/` — aoided, shellbridge, policy, CLI. The one layer every host inherits identically. |
| Opt-in branches, chosen per host | **dendrites** | `modules/dendrites/` — one tree, all branches (shipped + personal); each host enables the ones it wants. |
| Render surfaces that sound a rice | **facets** | `modules/facets/` — quickshell, stylix, compositor. The machinery that renders a song, reading its ricing elements from the songbook via `aoide.livery`. |

**Nucleus is the shared core every host inherits; dendrites are the opt-in branches each host chooses.** Facets are the *machinery* of ricing, the surfaces that sound a song, but they embed no ricing content: every palette, sound, icon, widget body, and the shipped standard rice (`sonata`) lives in `song/songbook/`; covers (wallpapers) live in the shared `song/covers/` library any song references by path (see [[Song-Anatomy]]). There is no separate "rice engine" layer. A song is *activated* by the walker (`lib/mkHost.nix` picks up the selected `rice.nix`), *resolved and emitted* by the native livery engine ([[livery]] — lint → resolve → emit → `stage/livery.json`), and *rendered* by the facets. All three layers live under `modules/` in `~/Aoide`, one walked module tree, the same grouping [[dxflake]] uses. Facets are the only nix that renders appearance, and each reads only `aoide.livery` and `aoide.arrangement` — no module reads another module.

## Mutation Policy

Radial distance from the nucleus encodes who may change a layer and how often:

- **Inherited machinery** (nucleus, facets) — change via upstream merge only.
- **New dendrite branches** — one host line to enable; growth is additive, so upstream merges stay conflict-free by construction.
- **`song/songbook/`** — the agent's writable domain: all ricing content (every song, its palettes/sounds/icons/widgets, and the shipped standard, `sonata`), gated by User approval at the rebuild step. Covers (wallpapers) are the one exception: shared assets in `song/covers/`, not per-song.

This is provenance, not path fences. Personal branches (`flatpak.nix`, `firefox.nix`, `jellyfin.nix`, …) grow in the same `modules/dendrites/` tree as upstream-shipped ones. Ownership is tracked by git merge-base; `aoide update` detects divergence from inherited files and warns.

## Dendritic Walker

The composition engine is an in-house dendritic walker: every file placed under a walked directory self-registers — no explicit import list needed. [[dxflake]] is the adoption target for the nucleus + dendrite import patterns. The walker replaces the `den` tool (pattern prior art only; not a dependency). As built (commit f3ceadf) it is `lib/walk.nix` — `listFilesRecursive` filtered to `.nix` files without a `/_` infix — and `lib/mkHost.nix` walks the whole `modules/` tree into each host and injects the discovered-packages overlay from the same `callPackage` paths the flake uses (see [[Codebase]]).

**Songs self-register on the same principle.** `lib/mkHost.nix` walks `song/songbook/` in addition to `modules/`. Each song's `rice.nix` guards itself with `lib.mkIf (config.aoide.song == "<name>")` — only the selected song activates per host. Committing a new song to the clone is sufficient to make it fleet-available; no import list needs updating. The `aoide.song` option (str, default `"sonata"`) selects the performance per host, set in `hosts/<host>/default.nix` as a one-line declaration. See [[Song-Vocabulary#Replay — any song, any host]] and [[Self-Ricing#Declare, Select, Replay]].

**Packages self-register too.** `pkgs/` is walked by `lib/pkgs.nix` (sibling to the module walker, same `_`-prefix shelving): dropping `pkgs/<name>/default.nix` registers it into the flake `packages` output, the host + vm overlays, and an auto-generated `pkg-<name>` check — from one source, no hand-list to edit (see [[Codebase]]).

## Repo Layout (high level)

```
~/Aoide/
├── modules/        the snowflake — three layers, walker-discovered
│   ├── nucleus/    core daemon, CLI, policy
│   ├── dendrites/  all branches: shipped + personal
│   └── facets/     quickshell · stylix · compositor
├── hosts/
│   ├── common/     cross-machine baseline
│   └── <host>/     machine-specific picks
├── pkgs/           aoide, hyprglass, kimi-code, melete, mneme (walker-discovered)
├── lib/            dendritic walker + helpers (checks ride as a flake output)
├── docs/
└── song/           the performed half (see [[Song-Vocabulary]])
```

The repo surface is only the subsystem (`modules/`) plus standard flake furniture (`hosts/`, `pkgs/`, `lib/`, `docs/`) and the `song/` content tree — no scaffolding sprayed across the root. Runtime dirs (`song/{stage,auditions}`, root `log/`, `index/`, `catalog/`) are gitignored and created at runtime, never committed. `hosts/` knows dendrites; dendrites never know hosts — the same separation [[dxflake]] enforces. Subfolders inside `modules/dendrites/` are grouping only; the walker registers every file regardless.

**The root is closed.** The directory list above is the whole surface — never invent a new top-level dir. New content lands inside the existing tree at its designated place: every per-song key, sound, icon, and widget body → `song/songbook/<song>/`; covers → the shared `song/covers/` library; module assets next to their module. The lookup for content paths is the Song Map ([[Song-Vocabulary#The Song Map]]); creating a new root directory is a contract change (`CONTRACTS.md` §2), not a convenience.

## Related

- [[dxflake]]
- [[Clone-and-Run]]
- [[livery]]
- [[Self-Ricing]]
- [[Codebase]]
