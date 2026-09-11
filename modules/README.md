# modules

The AoideOS module tree: everything a host needs beyond the core flake's own
`nixosModules.default`. Three layers, in the order the module system merges
them — `dendrites/` (opt-in), `facets/` (render surfaces), `nucleus/`
(unconditional core) — each with its own charter, in its own `README.md`.

## Named seams (what it exposes)

- `modules/default.nix` — the one aggregate this directory exposes to a
  host: `imports = [ ./dendrites ./facets ./nucleus ];` and nothing else.
  `lib/mkHost.nix` and `tests/vm-boot.nix` hand `../modules` to the module
  system as a single path; neither walks a file list.
- `dendrites/default.nix`, `facets/default.nix`, `nucleus/default.nix` —
  one aggregate per layer, each naming only the files inside its own
  directory.

## How `default.nix` composes

Every directory that holds modules carries exactly one `default.nix`,
listing its own entries and nothing outside itself — one line per file (or
subdirectory), in `LC_ALL=C` order, `_`-prefixed entries omitted. That order
is a contract, not taste: list-typed NixOS options merge in definition
order, so `modules/default.nix`'s `[ ./dendrites ./facets ./nucleus ]`
reproduces the same leaf order a reader gets from `ls -A | LC_ALL=C sort`
inside each directory. A new file is a new line in its own directory's
`default.nix`; deleting both removes the capability without a trace
elsewhere in the tree.

That guarantee is scoped to the leaves relative to each other: the tree
merges into a host's module list as ONE unit, at `modules/default.nix`'s
own import depth (nixpkgs collects imports breadth-first), so a host,
home-manager, or stylix module that also defines an order-sensitive list
option (`environment.systemPackages`, a `systemd.*` ordering list)
interleaves with the tree's contributions after it, not before.

`song/songbook/` is not a fourth layer here and carries no `default.nix` of
its own — a committed song is found by `lib/songbook.nix`'s own typed scan
(`song/songbook/<song>/rice.nix`), the one exception house rule 1 already
draws around `song/`.

## What it consumes

Nothing above itself. `modules/default.nix` is the whole surface a host
sees; the three layers below never read from outside their own directory
tree (root `AGENTS.md` house rule 5 is the one enumerated exception, for
facets reading `aoide.livery`/`aoide.arrangement`/`aoide.surfaces`).
