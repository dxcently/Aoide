# modules

The AoideOS module tree: everything a host needs beyond the core flake's own
`nixosModules.default`. Four directories — `nucleus/` (unconditional core),
`dendrites/` (opt-in lanes), and the two records read beside them,
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
  body imported at catalogue time. `lib/composition.nix`
  reads this record before any module graph exists and imports only what
  selection kept.
- `dendrites/default.nix`, `nucleus/default.nix` —
  one aggregate per layer. `dendrites/default.nix` names no dendrite: it derives
  its imports from the catalogue
  (`builtins.attrValues (import ../default.nix).catalogue`, and a provider
  registry entry contributes every alternative). `nucleus/default.nix` names only
  the files inside its own directory — it is the unconditional core, so it has no
  catalogue name to be found by. `lib/composition.nix` imports the two
  aggregates for a host that took the whole tree, and `tests/vm-boot.nix`
  imports them directly as a consumer would;
  nothing imports the catalogue as a module.
- A dendrite file is a lane record (`{ body; nixos; }`, CONTRACTS.md §2):
  `body` is the module this tree merges (its `aoide.<name>.*` options and its
  guard), `nixos` is the module the constructor imports for a host that
  selected it. `dendrites/default.nix` imports every `body`. That aggregate and
  a SELECTED lane are mutually exclusive in one module list — both import the
  same `body`, so `aoide.<name>.enable` arrives twice and nixpkgs throws
  `already declared`; `mkNixosModules` refuses the pair by name.

## How the aggregates compose

Every directory that holds modules carries exactly one `default.nix`, listing
its own entries and nothing outside itself — one line per file (or
subdirectory), in `LC_ALL=C` order, `_`-prefixed entries omitted. The one
exception is `dendrites/`, whose aggregate names nothing and derives its imports
from the catalogue: a dendrite's ONE line is its catalogue line, and the full
tree follows it.

That order is a contract, not taste: list-typed NixOS options merge in
definition order, so `[ ./dendrites ./nucleus ]` reproduces the same
leaf order a reader gets from `ls -A | LC_ALL=C sort` inside each directory. The
dendrite aggregate's order is the catalogue's attribute-name order instead —
`LC_ALL=C` too, and the two agree for every name in the tree, but they diverge
the day a name and its filename stop sorting alike (`foo.nix` beside
`foo-bar.nix`). The catalogue is that layer's authority: the file that names a
capability names its order with it.

A new dendrite is a new file plus one catalogue line; a new nucleus
module is a new file plus one line in its own directory's `default.nix`.
Deleting both removes it without a trace elsewhere in the tree. `_`-prefix
shelving is the same act short of the deletion: a `_`-prefixed file is never
catalogued and never listed, so nothing imports it, and the `_` marks it as
parked rather than deleted.

That guarantee is scoped to the leaves relative to each other: the three
aggregates arrive as separate import paths, in that order, so a host,
home-manager, or stylix module that also defines an order-sensitive list
option (`environment.systemPackages`, a `systemd.*` ordering list) interleaves
with the tree's contributions at its own position in the host's module list.

`song/songbook/` is not a fourth layer here and carries no `default.nix` of
its own — a committed song is found by `lib/songbook.nix`'s own typed scan
(`song/songbook/<song>/rice.nix`), the one exception house rule 1 already
draws around `song/`.

## What it consumes

Nothing above itself. The catalogue plus the two aggregates are the whole
surface a host sees; the layers below never read from outside their own
directory tree (root `AGENTS.md` house rule 5 is the one enumerated exception,
for paint lanes reading `aoide.livery`/`aoide.arrangement`/`aoide.surfaces`
plus the core scalars and the song selection).
