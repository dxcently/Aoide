# AGENTS.md — invariants across `modules/{nucleus,dendrites,aggregations,overrides}/`

Cross-module rules only. A directory's own `AGENTS.md` holds what's local to
it; this file holds what would otherwise be repeated in each of the four. Points
up to the root `AGENTS.md` for house rule 5 (the closed paint-read whitelist),
house rule 1 (a rice agent writes only `song/songbook/<song>/`; `modules/`
changes only on a lane the User ordered, whose scope is named, or additively as
a new dendrite plus its catalogue line) and house rule 7 (everything is
a plugin).

## The catalogue names; one aggregate derives

`modules/default.nix` is the catalogue — plain data, never a module.
`catalogue` holds one `name = path;` line per dendrite, and that name is how a
host, a user and an aggregation reach it. It is the ONE place a dendrite file is
named. `aggregations` and `overrides` are the records read one level deep beside
it — by their own directories' discovery files, names and paths only, never a
body at catalogue time. A
dendrite is added as a new file plus ONE line — the catalogue line — and removed
by deleting both, with no other file in the tree aware it existed.

## A dendrite is a plain module

A dendrite file is an ordinary NixOS module (CONTRACTS.md §2). It declares
`aoide.<name>.*`, sets that flag `lib.mkDefault true` and guards its config with
`aoide.<name>.enable`; what it sets under `habit.home` is its home half. The
constructor imports the files of what a host selected, and only those.

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
directory is what names it. The dendrites have no such file: the catalogue is
the only list of them, in the `LC_ALL=C` order it is written in. Nothing walks
the dendrite tree: a name with no catalogue line is unreachable, which is what
shelving means.

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
  lands, a toggle's default changes, a dendrite's shape changes, or a
  read-whitelist entry is added.
- `CONTRACTS.md` §2 when the dendrite shape itself moves.
- Root `AGENTS.md` house rule 5 if the whitelist itself grows — that's the
  one repo-wide invariant this file only restates.
