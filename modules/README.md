# modules

The AoideOS module tree: everything a host needs beyond the core flake's own
`nixosModules.default`. Three layers, in the order the module system merges
them — `dendrites/` (opt-in), `facets/` (render surfaces), `nucleus/`
(unconditional core) — each with its own charter, in its own `README.md`, plus
the registry that names what can be selected.

## Named seams (what it exposes)

- `modules/default.nix` — the registry. Plain data, not a module:
  `catalogue` names every dendrite once, by the name a host selects it with,
  and points at the file (or directory) that answers it; `aggregations` and
  `overrides` are the sibling discovery records. `lib/composition.nix` reads
  this record before any module graph exists and imports only what selection
  kept.
- `dendrites/default.nix`, `facets/default.nix`, `nucleus/default.nix` —
  one aggregate per layer, each naming only the files inside its own
  directory. `lib/mkHost.nix` and `tests/vm-boot.nix` import the three
  aggregates directly; nothing imports the registry as a module.
- A dendrite file is a lane record (`{ body; nixos; }`, CONTRACTS.md §2):
  `body` is the module this tree merges (its `aoide.<name>.*` options and its
  guard), `nixos` is the module the constructor imports for a host that
  selected it. `dendrites/default.nix` imports every `body`.

## How the aggregates compose

Every directory that holds modules carries exactly one `default.nix`,
listing its own entries and nothing outside itself — one line per file (or
subdirectory), in `LC_ALL=C` order, `_`-prefixed entries omitted. That order
is a contract, not taste: list-typed NixOS options merge in definition order,
so `[ ./dendrites ./facets ./nucleus ]` reproduces the same leaf order a
reader gets from `ls -A | LC_ALL=C sort` inside each directory. A new file is
a new line in its own directory's `default.nix`; deleting both removes the
capability without a trace elsewhere in the tree. The catalogue line beside it
is what makes the capability *selectable*; the aggregate line is what puts it
in the full tree.

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

Nothing above itself. The catalogue plus the three aggregates are the whole
surface a host sees; the layers below never read from outside their own
directory tree (root `AGENTS.md` house rule 5 is the one enumerated exception,
for facets reading `aoide.livery`/`aoide.arrangement`/`aoide.surfaces`).
