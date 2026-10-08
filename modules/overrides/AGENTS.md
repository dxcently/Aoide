# AGENTS.md — invariants for `modules/overrides/`

Points up to `modules/AGENTS.md` and root `AGENTS.md` (house rule 5 — a record
is not an exemption from the closed read whitelist; it is a fix, applied where
the capability was selected).

## A record's fields are closed

`dendrites`, `hosts`, `overlay`, `system`, `home` — that is the whole set
(`overrideFields` in habit's composition). An unknown field is an error, not
an extension point: adding a field means amending habit's constructor, then this
list in the commit that takes the new habit, and saying why in the record's own
comment.

`dendrites` is required and must name catalogue names. A record with no target,
or a target the catalogue does not hold, fails by name — a fix that applies to
nothing looks exactly like a fix that works.

## Keep the work inside the functions

`overlay`, `system` and `home` are functions; an unmatched record's functions are
never called, and that is the only boundary this directory has. Anything that
must not be evaluated for a host that did not match belongs INSIDE one of those
functions. A record-level binding that computes a package set is evaluated on
every host, and defeats the seam it lives in.

## Never a second naming site

`modules/overrides/default.nix` names the records, by file name. Nothing else
names a record file — not `flake.nix`, not a host, not another module.
Deleting a file plus nothing else removes the fix: that is what makes it a
plugin.

## What needs a docs update in the same commit

- This directory's `README.md` when the record shape changes, the matching rule
  changes, or the first real record lands (the "Today" section is a claim about
  the current tree).
- `CONTRACTS.md` §0 when the repo root's shape changes — the `overrides` line of
  the closed root list lives there.
- `modules/default.nix` never needs a line for a record: the directory's own
  discovery is the naming site.
