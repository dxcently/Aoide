# modules

The AoideOS module tree: everything a host needs beyond the core flake's own
`nixosModules.default`. Four directories — `nucleus/` (unconditional core),
`dendrites/` (opt-in capabilities), and the two records read beside them,
`aggregations/` (named groups of catalogue entries) and `overrides/`
(capability-scoped fixes, applied to the hosts that selected the capability) —
each with its own charter, in its own `README.md`, plus
the catalogue that names what can be selected.

## Named seams (what it exposes)

- `modules/default.nix` — the catalogue. Plain data, not a module:
  `catalogue` names every dendrite once, by the name a host selects it with,
  and points at the file (or directory) that answers it. It is the ONE place a
  dendrite file is named. `aggregations` and `overrides` are the sibling
  discovery records, read one level deep beside it — names and paths only, no
  body imported at catalogue time. Habit's composition
  reads this record before any module graph exists and imports only what
  selection kept.
- `nucleus/default.nix` — the unconditional core's aggregate. It names only the
  files inside its own directory, and has no catalogue name to be found by.
  `lib/aoideos.nix` hands it to the constructor as an `extraModules` entry, and
  `nixosModules.nucleus` exports it for a consumer's own `extraModules`.
  Nothing imports the catalogue as a module.
- A dendrite file is a plain module (CONTRACTS.md §2): its `aoide.<name>.*`
  options, its guard, and the flag set `lib.mkDefault true`, with its Home
  Manager half under `habit.home`. The constructor imports the file only for a
  host that selected it.

## How the layers compose

`nucleus/` carries one `default.nix`, listing its own entries and nothing
outside itself — one line per file, in `LC_ALL=C` order, `_`-prefixed entries
omitted. `dendrites/` carries none: a dendrite's ONE line is its catalogue line,
written in `LC_ALL=C` order, and the constructor imports the files selection
kept.

A new dendrite is a new file plus one catalogue line; a new nucleus
module is a new file plus one line in its own directory's `default.nix`.
Deleting both removes it without a trace elsewhere in the tree. `_`-prefix
shelving is the same act short of the deletion: a `_`-prefixed file is never
catalogued and never listed, so nothing imports it, and the `_` marks it as
parked rather than deleted.

That guarantee is scoped to the leaves relative to each other: the selected
dendrites, the nucleus (an `extraModules` entry, which habit places after the
dendrites) and the host arrive as separate module-list entries, so a module that
also defines an order-sensitive list option (`environment.systemPackages`, a
`systemd.*` ordering list) interleaves with the tree's contributions at its own
position in the host's module list.

`song/songbook/` is not a fourth layer here and carries no `default.nix` of
its own — a committed song is found by `lib/songbook.nix`'s own typed scan
(`song/songbook/<song>/rice.nix`), the one exception house rule 1 already
draws around `song/`.

## What it consumes

Nothing above itself. The catalogue plus the nucleus aggregate are the whole
surface a host sees; the layers below never read from outside their own
directory tree (root `AGENTS.md` house rule 5 is the one enumerated exception,
for paint lanes reading `aoide.livery`/`aoide.arrangement`/`aoide.surfaces`
plus the core scalars and the song selection).
