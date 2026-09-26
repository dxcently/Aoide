# AGENTS.md — invariants across `modules/{dendrites,nucleus}/` and the registry

Cross-module rules only. A directory's own `AGENTS.md` holds what's local to
it; this file holds what would otherwise be repeated in all three. Points up
to the root `AGENTS.md` for house rule 5 (the closed paint-read whitelist),
house rule 1 (only `song/` is agent-writable — `modules/` changes by
upstream merge or new-dendrite-addition only) and house rule 7 (everything is
a plugin).

## The catalogue names; one aggregate derives

`modules/default.nix` is the catalogue — plain data, never a module.
`catalogue` holds one `name = path;` line per dendrite, and that name is how a
host, a user and an aggregation reach it. It is the ONE place a dendrite file is
named. `aggregations` and `overrides` are the records read one level deep beside
it — empty until `modules/aggregations/` and `modules/overrides/` land. A
dendrite is added as a new file plus ONE line — the catalogue line — and removed
by deleting both, with no other file in the tree aware it existed:
`modules/dendrites/default.nix` derives its imports from the catalogue
(`builtins.attrValues (import ../default.nix).catalogue`) instead of naming a
file itself.

## A dendrite is a lane record

A dendrite file evaluates to `{ body; nixos; }` (CONTRACTS.md §2). `body`
declares `aoide.<name>.*` and guards its config with `aoide.<name>.enable`;
`nixos` imports `body` and sets that flag `lib.mkDefault true`. The tree
imports bodies; the constructor imports the `nixos` lane of what a host
selected. Nothing else reads a lane, and a lane is never imported by
`modules/dendrites/default.nix`.

## Flags default off

Every dendrite (and every paint feature behind a toggle) gates on its own
`aoide.<name>.enable`, and that default is `false` unless the module is
core plumbing discovered unconditionally (`modules/nucleus/*`, which has no
`mkIf` guard by design — see `options.nix`'s header). A dendrite ships
disabled; a host opts in with one line in `hosts/<host>/default.nix`.

## Flat option assignment

Write host and module options one per line — `aoide.enable = true;`,
`aoide.song = "sonata";` — never gathered into `aoide = { … }`. The two are
identical to nix; the flat form greps and diffs cleanly, and enabling a
capability stays one copyable line. `statix.toml` disables `repeated_keys`
for this reason, so the linter does not fight the style.

## Aggregate discipline

`modules/nucleus/default.nix` names every
file in its own directory, one line per file, in `LC_ALL=C` order, and nothing
from outside it — the core is nothing selectable, so its own
directory is what names it. `modules/dendrites/default.nix` is the other half of
the rule: it names no dendrite at all and derives its imports from the
catalogue, in attribute-name order — the `LC_ALL=C` order the catalogue itself
is written in. Nothing walks the dendrite tree: a name with no catalogue line is
unreachable, which is what shelving means.

## `_`-prefix shelving

A `_`-prefixed file or directory is not a module: nothing imports it and the
catalogue does not name it — the opt-out for work-in-progress or scratch
dendrites: `modules/dendrites/_example.nix` is the checked-in template. Drop the
leading `_` and add one catalogue line; drop that line and add the `_` back to
shelve without deleting.

## The closed read whitelist

Root `AGENTS.md` house rule 5 owns the list — read it there, by reference,
never restated here: one list, one place to amend, or the two drift.
`lib/checks.nix` is where a future automated coupling check would live; today
this is a code-review discipline, not a build failure.

## What needs a docs update in the same commit

- The owning directory's `README.md`/`AGENTS.md` when a new dendrite
  lands, a toggle's default changes, a lane's shape changes, or a
  read-whitelist entry is added.
- `CONTRACTS.md` §2 when the dendrite shape itself moves.
- Root `AGENTS.md` house rule 5 if the whitelist itself grows — that's the
  one repo-wide invariant this file only restates.
