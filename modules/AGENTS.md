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

## Flat option assignment

Write host and module options one per line — `aoide.enable = true;`,
`aoide.song = "sonata";` — never gathered into `aoide = { … }`. The two are
identical to nix; the flat form greps and diffs cleanly, and enabling a
capability stays one copyable line. `statix.toml` disables `repeated_keys`
for this reason, so the linter does not fight the style.

## Aggregate discipline

Each of `modules/dendrites/`, `modules/facets/`, `modules/nucleus/` carries
its own `default.nix`, naming every file in that directory one line per
file, in `LC_ALL=C` order — **adding a capability is a new file plus one
line in that directory's own `default.nix`, never an edit to a list
anywhere else**. `modules/default.nix` imports the three directory
aggregates and nothing else; no file is ever named from outside the
directory that holds it. A module self-gates on its own
`aoide.<name>.enable`; nothing outside the module switches it on.

## `_`-prefix shelving

A `_`-prefixed file or directory is not a module: nothing imports it and no
`default.nix` lists it — the opt-out for work-in-progress or scratch
dendrites: `modules/dendrites/_example.nix` is the checked-in template. Drop
the leading `_` and add one line to that directory's `default.nix` to
activate; drop the line and add the `_` back to shelve without deleting.

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
