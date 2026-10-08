---
type: concept
created: 2026-07-25
updated: 2026-08-27
tags: [aoide, architecture, nix]
source: "[[references/AOIDE-HANDOFF]]"
---

# Snowflake Anatomy — the Frozen Half

Aoide names its nix layer after snowflake morphology — see [[Lexicon#The frozen family — snowflake morphology]] for why.

## The Layers

| Layer | Morphology term | What it is |
|---|---|---|
| Core shared by every host | **nucleus** | `modules/nucleus/` — aoided, secrets, policy, the CLI doors. The one layer every host inherits identically. |
| Opt-in branches, chosen per host | **dendrites** | `modules/dendrites/` — one tree, all branches (shipped + personal); each host enables the ones it wants, by name. |
| Named groups a host takes in one line | **aggregations** | `modules/aggregations/` — memberships over catalogue names (`base`, `desktop`, `agents`, `aoideos`); inert data, no options of their own. |
| A repair that travels with a capability | **overrides** | `modules/overrides/` — capability-scoped fixes, applied to the hosts that selected the capability; empty today. |

**Nucleus is the shared core every host inherits; dendrites are the opt-in branches each host chooses.** The branches that *render* — the paint lanes `quickshell`, `stylix`, `compositor`, `greeter`, `lyra` — are the machinery of ricing: the surfaces that sound a song, embedding no ricing content. Every palette, sound, icon, widget body, and the shipped standard rice (`sonata`) lives in `song/songbook/`; covers (wallpapers) live in the shared `song/covers/` library any song references by path (see [[Song-Anatomy]]). There is no separate "rice engine" layer. A song is *selected* by the host module (`habit.song.declared`; `habit.song.available` is what the host builds in to stage), *resolved and emitted* by the native livery engine ([[livery]] — lint → resolve → emit → `stage/livery.json`), and *rendered* by the paint lanes. All four module directories live under `modules/` in `~/Aoide`, the same grouping [[dxflake]] adopts. Paint lanes are the only nix that renders appearance, and each reads only the namespaces root `AGENTS.md` house rule 5 enumerates — no module reads another module.

## Mutation Policy

Radial distance from the nucleus encodes who may change a layer and how often:

- **Inherited machinery** (nucleus, aggregations, overrides, and every lane the repo ships) — changes only on a lane the User ordered, with that lane's scope named.
- **New dendrite branches** — one catalogue line plus one file; growth is additive, so upstream merges stay conflict-free by construction.
- **`song/songbook/`** — a rice agent's writable domain: all ricing content (every song, its palette/sounds/icons/widgets, and the shipped standard, `sonata`), gated by User approval at the rebuild step. Covers (wallpapers) are the one exception: shared assets in `song/covers/`, not per-song.

This is provenance, not path fences. Personal branches (`flatpak.nix`, `firefox.nix`, `jellyfin.nix`, …) grow in the same `modules/dendrites/` tree as upstream-shipped ones. Ownership is tracked by git merge-base; `aoide update` detects divergence from inherited files and warns.

## Discovery without a walker

Nothing walks a module tree. `modules/default.nix` is the CATALOGUE — plain data, read before any module graph exists: one `name = path;` line per dendrite, and that line is both what makes a capability selectable and what puts it in the full tree. The constructor imports exactly the files selection kept from it, so a file with no catalogue line is unreachable, which is what shelving means. `aggregations/` and `overrides/` are found one level deep by their own directories' `default.nix` — a directory name is enough for inert data. [[dxflake]] is the adoption target for the nucleus + dendrite import patterns. The catalogue replaces the `den` tool (pattern prior art only; not a dependency) and the dendritic walker it briefly had (`lib/walk.nix`, retired): a module is now NAMED once rather than discovered by sitting in a directory.

**Songs are selected, not walked.** A host names what it performs (`habit.song.declared`) and what it keeps built in to stage without a rebuild (`habit.song.available`); `lib/songbook.nix` finds `song/songbook/<song>/` by one typed scan and `lib/aoideos.nix` wires exactly the selected songs' `rice.nix` files in — the same "selection before evaluation" rule the module tree follows. Each song's `rice.nix` still guards itself with `lib.mkIf (config.aoide.song == "<name>")`, so only the performed song activates. Committing a new song to the clone makes it *available to select*, not fleet-live; it is built in on the hosts that name it, and a host may stage a song it never built in when that song's nix and packages are present. A host that names nothing performs no song. See [[Song-Vocabulary#Replay — any song, any host]] and [[Self-Ricing#Declare, Select, Replay]].

**Packages self-register on the same principle.** `pkgs/` is discovered by `lib/pkgs.nix` (sibling to the catalogue's naming, same `_`-prefix shelving): dropping `pkgs/<name>/default.nix` registers it into the flake `packages` output, the host + vm overlays, and an auto-generated `pkg-<name>` check — from one source, no hand-list to edit (see [[Codebase]]).

## Repo Layout (high level)

```
~/Aoide/
├── modules/        the snowflake — four directories, the catalogue names what is selectable
│   ├── nucleus/    core daemon, CLI, policy
│   ├── dendrites/  all branches: shipped + personal
│   ├── aggregations/  named groups a host takes in one line
│   └── overrides/  capability-scoped fixes
├── hosts/          one record per machine (`_`-prefixed = template)
├── pkgs/           aoide, lyra-shell, lyra-songbook, hyprglass, melete, mneme (discovered)
├── lib/            constructor, composition, checks, discovery (checks ride as a flake output)
├── docs/
└── song/           the performed half (see [[Song-Vocabulary]])
```

The repo surface is only the subsystem (`modules/`) plus standard flake furniture (`hosts/`, `pkgs/`, `lib/`, `docs/`) and the `song/` content tree — no scaffolding sprayed across the root. Runtime trees (`song/stage/`, `song/declared/`, `state/`, `run/qml/`, the composed `songbook/`, the audit `log/`) hang off one runtime root — `$AOIDE_ROOT`, the `aoide.root` option, default `~/.aoide` — created at runtime, never committed; the checkout carries no runtime state. `hosts/` knows dendrites; dendrites never know hosts — the same separation [[dxflake]] enforces. Subfolders inside `modules/dendrites/` are grouping only; a file becomes a dendrite by its one catalogue line.

**The root is closed.** The directory list above is the whole surface — never invent a new top-level dir. New content lands inside the existing tree at its designated place: every per-song key, sound, icon, and widget body → `song/songbook/<song>/`; covers → the shared `song/covers/` library; module assets next to their module. The lookup for content paths is the Song Map ([[Song-Vocabulary#The Song Map]]); creating a new root directory is a contract change (`CONTRACTS.md` §2), not a convenience.

## Related

- [[dxflake]]
- [[Clone-and-Run]]
- [[livery]]
- [[Self-Ricing]]
- [[Codebase]]
