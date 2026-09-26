# AGENTS.md — invariants across `modules/{dendrites,facets,nucleus}/` and the registry

Cross-module rules only. A directory's own `AGENTS.md` holds what's local to
it; this file holds what would otherwise be repeated in all three. Points up
to the root `AGENTS.md` for house rule 5 (the closed facet-read whitelist),
house rule 1 (only `song/` is agent-writable — `modules/` changes by
upstream merge or new-dendrite-addition only) and house rule 7 (everything is
a plugin).

## The registry names; the aggregates import

`modules/default.nix` is the registry — plain data, never a module.
`catalogue` holds one `name = path;` line per capability, and that name is
how a host, a user and an aggregation reach it; `aggregations` and `overrides`
are discovered one level deep beside it. A capability's own directory still
carries the aggregate that puts it in the full tree
(`modules/dendrites/default.nix` imports every dendrite's `body`), so a
capability is added as a new file plus two lines — one in the catalogue, one
in the aggregate — and removed by deleting both, with no other file in the
tree aware it existed.

## A dendrite is a lane record

A dendrite file evaluates to `{ body; nixos; }` (CONTRACTS.md §2). `body`
declares `aoide.<name>.*` and guards its config with `aoide.<name>.enable`;
`nixos` imports `body` and sets that flag `lib.mkDefault true`. The tree
imports bodies; the constructor imports the `nixos` lane of what a host
selected. Nothing else reads a lane, and a lane is never imported by
`modules/dendrites/default.nix`.

## Flags default off

Every dendrite (and every facet feature behind a toggle) gates on its own
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

Each of `modules/dendrites/`, `modules/facets/`, `modules/nucleus/` carries
its own `default.nix`, naming every file in that directory one line per
file, in `LC_ALL=C` order, and nothing named from outside that directory. The
catalogue is the one place a dendrite is named by its *selection* name — the
registry is not a second aggregate, it is the list of what can be chosen. No
file is ever named from outside the directory that holds it, catalogue entries
included: a catalogue line names a file inside `modules/dendrites/`, written by
the directory that holds it.

## `_`-prefix shelving

A `_`-prefixed file or directory is not a module: nothing imports it and no
`default.nix` lists it — the opt-out for work-in-progress or scratch
dendrites: `modules/dendrites/_example.nix` is the checked-in template. Drop
the leading `_`, add one line to the catalogue and one to
`modules/dendrites/default.nix`; drop both and add the `_` back to shelve
without deleting.

## The closed read whitelist

No module reads another module. Facets read exactly `aoide.livery`,
`aoide.arrangement`, and `aoide.surfaces` (root `AGENTS.md` house rule 5) —
an enumerated, closed set, never `aoide.*` wholesale. `lib/checks.nix` is
where a future automated coupling check would live; today this is a
code-review discipline, not a build failure.

## What needs a docs update in the same commit

- The owning directory's `README.md`/`AGENTS.md` when a new dendrite/facet
  lands, a toggle's default changes, a lane's shape changes, or a
  read-whitelist entry is added.
- `CONTRACTS.md` §2 when the dendrite shape itself moves.
- Root `AGENTS.md` house rule 5 if the whitelist itself grows — that's the
  one repo-wide invariant this file only restates.
