# AGENTS.md — modules/facets

Points up to `modules/AGENTS.md` for cross-module invariants (walk
discipline, `_`-prefix shelving, the closed read whitelist) — this file
covers only what's specific to facets.

## Invariants

- **Facets read `aoide.livery`/`aoide.arrangement`/`aoide.surfaces` and
  nothing else.** No facet reads another facet, a dendrite, or any other
  `aoide.*` option namespace — the whitelist is enumerated and closed (root
  `AGENTS.md` house rule 5).
- **The paint test decides QML placement, not convenience.** A file stays
  in `modules/facets/quickshell/qml/` only if it's song-blind, song-plural,
  and a bridge/mechanism (CONTRACTS.md §0, "The paint test"). A shared
  visual component that fails any leg belongs in a song's `widgets/`
  instead — the facet is not a component library.
- **`quickshell/` never reads `song/` runtime paths at build time** —
  `checks.no-song-read` enforces this structurally, not just by
  convention.
- **`compositor/` is LOOK + session plumbing only.** Host-invariant
  behavior (keybinds, input devices, tiling layout, misc window rules)
  belongs in `modules/dendrites/hyprland.nix` so a re-rice can't disturb it.

## Extension points

- **A new facet** is a new directory under `modules/facets/` with its own
  `default.nix`, reading only the closed whitelist above — discovered by
  `lib/walk.nix` automatically, no import-list edit.
- **A new surface inside `quickshell/`** runs the paint test before
  landing; if it fails, it belongs in a song's `widgets/` instead.

## Docs update required in the same commit

- This `README.md` when a facet's scope, surfaces owned, or read set
  changes.
- `CONTRACTS.md §0`/`§1` if the paint test or the read whitelist itself
  changes.
