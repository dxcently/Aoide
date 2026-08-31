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
- **A surface takes its size from its CONTENT; content never sizes itself
  from the SCREEN.** A layer anchors only the edges it genuinely occupies
  and lets `implicitWidth`/`implicitHeight` follow what it draws (the
  herald anchors bottom+right and sizes off its own card stack; the bar
  anchors top+left+right at a fixed height). A full-screen overlay is
  legitimate for a modal (launcher, powermenu), but its CONTENT is then
  naturally sized and centred — never anchored to the layer's own edges,
  and never scaled off `screen.height`. A card keeps its own aspect
  ratio; a panel derives from what it stacks, capped, not from a screen
  fraction. **Every geometry constant is a claim about a screen you have
  not seen** — a portrait panel, a second monitor, a different scale.
  Surface-sized content hides its own breakage: on the screen it was
  written for, the derived number sits near what the content wanted, so
  it reads as correct until the geometry changes and every
  height-derived value inflates while every width-derived one clips.
  Verify a new surface at more than one aspect before landing it.
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
