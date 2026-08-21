# AGENTS.md — invariants across `modules/{nucleus,facets,dendrites}/`

Cross-module rules only. A directory's own `AGENTS.md` holds what's local to
it; this file holds what would otherwise be repeated in all three. Points up
to the root `AGENTS.md` for house rule 5 (the closed facet-read whitelist)
and house rule 1 (only `song/` is agent-writable — `modules/` changes by
upstream merge or new-dendrite-addition only).

## Flags default off

Every dendrite (and every facet feature behind a toggle) gates on its own
`aoide.<name>.enable`, and that default is `false` unless the module is
core plumbing discovered unconditionally (`modules/nucleus/*`, which has no
`mkIf` guard by design — see `options.nix`'s header). A dendrite ships
disabled; a host opts in with one line in `hosts/<host>/default.nix`.

## Walk discipline

`lib/walk.nix` discovers every `.nix` file under `modules/dendrites/` and
`modules/facets/*` by recursive filesystem walk — **adding a capability is a
new file, never an edit to an import list**. A module self-gates on its own
`aoide.<name>.enable`; nothing outside the module switches it on.

## `_`-prefix shelving

A path containing `/_` (a filename or a directory) is skipped by the
walker's filter (`lib/walk.nix`) — the opt-out for work-in-progress or
scratch dendrites: `modules/dendrites/_example.nix` is the checked-in
template. Drop the leading `_` to activate; add it to shelve without
deleting.

## The closed read whitelist

No module reads another module. Facets read exactly `aoide.livery`,
`aoide.arrangement`, and `aoide.surfaces` (root `AGENTS.md` house rule 5) —
an enumerated, closed set, never `aoide.*` wholesale. `lib/checks.nix` is
where a future automated coupling check would live; today this is a
code-review discipline, not a build failure.

## What needs a docs update in the same commit

- The owning directory's `README.md`/`AGENTS.md` when a new dendrite/facet
  lands, a toggle's default changes, or a read-whitelist entry is added.
- Root `AGENTS.md` house rule 5 if the whitelist itself grows — that's the
  one repo-wide invariant this file only restates.
