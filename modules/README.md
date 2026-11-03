# modules

The Strillix module tree: everything a host needs beyond the core flake's own
`nixosModules.default`. Four directories — `nucleus/` (the unconditional option contract),
`dendrites/` (opt-in capabilities), and the two records read beside them,
`aggregations/` (named groups of catalogue entries) and `overrides/`
(capability-scoped fixes, applied to the hosts that selected the capability) —
each with its own charter, in its own `README.md`, plus
the catalogue that names what can be selected.

## Named seams (what it exposes)

- `modules/default.nix` — the catalogue. Plain data, not a module:
  `catalogue` names every dendrite once, by the name a host selects it with,
  and points at the file (or directory) that answers it, or, for a component
  slot, holds the provider set inline, or, for a dendrite written as a NixOS
  module and a Home Manager module, holds a `{ byClass = …; }` marker
  (below). It is the ONE place a dendrite file is named. `aggregations` and
  `overrides` are the sibling discovery records, read one level deep beside
  it — names and paths only, no body imported at catalogue time. Habit's
  composition reads this record before any module graph exists and imports
  only what selection kept.
- `nucleus/default.nix` — the option contract's aggregate. It names only the
  files inside its own directory, and has no catalogue name to be found by.
  `lib/strillix.nix` hands it to the constructor as an `extraModules` entry, and
  `nixosModules.nucleus` exports it for a consumer's own `extraModules`. It
  declares options and assertions and configures nothing; the services every
  host carries are catalogue entries, members of the `nucleus` aggregation.
  Nothing imports the catalogue as a module.
- A dendrite file is a plain module (CONTRACTS.md §2): the `aoide.<name>.*`
  options its body reads, its unconditional body, and its Home Manager half under
  `habit.home`. The constructor imports the file only for a
  host that selected it. A dendrite may instead be a NixOS module and a Home Manager
  module written together, named by a `{ byClass = …; }` marker (below).

## How the layers compose

`nucleus/` carries one `default.nix`, listing its own entries and nothing
outside itself — one line per file, in `LC_ALL=C` order, `_`-prefixed entries
omitted. `dendrites/` carries none: a dendrite's ONE line is its catalogue line,
written in `LC_ALL=C` order, and the constructor imports the files selection
kept.

A new dendrite is a new file plus one catalogue line (and one member line in
`aggregations/nucleus` if every host carries it); a new contract file is a new
file plus one line in `nucleus/default.nix`.
Deleting both removes it without a trace elsewhere in the tree. `_`-prefix
shelving is the same act short of the deletion: a `_`-prefixed file is never
catalogued and never listed, so nothing imports it, and the `_` marks it as
parked rather than deleted.

That guarantee is scoped to the leaves relative to each other: the selected
dendrites, the contract (an `extraModules` entry, which habit places after the
dendrites) and the host arrive as separate module-list entries, so a module that
also defines an order-sensitive list option (`environment.systemPackages`, a
`systemd.*` ordering list) interleaves with the tree's contributions at its own
position in the host's module list.

`rices/` is not a fourth layer here and carries no `default.nix` of
its own — a committed rice is found by `lib/rices.nix`'s own typed scan
(`rices/<rice>/rice.nix`), the one exception house rule 1 already
draws around `rice/`.

## The byClass marker

A dendrite may be written as a NixOS module and a Home Manager module in one place:
`{ byClass = X; }`, where X is a file (or a directory's `default.nix`), or the record
inline. The record is `{ nixos = <module>; homeManager = <module>; }`, with at least one
of the two keys and no other. The NixOS module applies where the host is built, and the
Home Manager module goes to each user's Home Manager. The marker stays data in the
catalogue, and `lib/by-class.nix` renders it before habit reads the registry. The
rendered entry is a function that imports X only when a host selects the entry, so an
unselected one is never read.

The record shape exists so a dendrite file works without habit: a flake without habit
imports `X.nixos` or `X.homeManager` directly. The marker is how a habit registry tells
the shape apart without reading the file.

A marker stands in two places, and only in these: a catalogue line
(`name = { byClass = X; };`), or the target of an inline provider set
(`name = { providers.<p> = { byClass = X; }; };`). A provider set written as a file, a
directory whose `default.nix` is `{ providers = …; }`, holds none: a marker inside it
is not rendered, because rendering it would import that file for every host. A
provider set whose target needs a marker is written inline on the registry line.

The inventory names a marker's value as `<value '<name>'>`, not as its file. A refusal
of the value (a file that is not `{ nixos; homeManager; }`) names the file, or the inline
catalogue entry.

## What it consumes

Nothing above itself. The catalogue plus the contract aggregate are the whole
surface a host sees; the layers below never read from outside their own
directory tree (root `AGENTS.md` house rule 5 is the one enumerated exception,
for paint lanes reading `strillix.theme`/`strillix.layout`/`strillix.surfaces`
plus the core scalars and the rice selection).
