# manifest.nix — the rices generator, as SHIPPED (S9, concepts/rice/
# Self-Ricing.md "the runtime reads").
#
# This file is the entry `strillix-rice::widgets` evaluates with a plain
# `nix-instantiate --eval --strict --json` over a FILE:
#
#   nix-instantiate --eval --strict --json --expr \
#     'import <templates>/../nix/manifest.nix { rices = <AOIDE_ROOT>/rices; }'
#
# No flake, no network, no checkout, no `--impure`. What a repo-less machine
# regenerating its OWN rices needs, and all it needs.
#
# It sits beside its three dependencies, all copied in by
# `pkgs/strillix-rices/default.nix`:
#
#   rices.nix   `lib/rices.nix`, verbatim — THE generator
#   rice.nix       `lib/rice.nix`, verbatim — `composeRice`, which the shelf
#                  records reach through
#   lib/           the locked nixpkgs `lib/`, tests dropped
#
# The copies are copies: `lib/rices.nix` is the one generator, and every
# caller — this entry, the strillix lane's `quickshellConfig` derivation, the
# `ricesManifest` flake output — evaluates the same file, so a machine
# running this cannot drift from the build that deployed it.
#
# ── What it returns ────────────────────────────────────────────────────────
#   manifest   `<rice>` → `{ <slot> = { owner; file; }; }`, rices with no slot
#              omitted (the same asymmetry `ricesManifest` carries)
#   registry   `<rice>` → that rice's widget-TYPE registry entry, EVERY rice
#              including the empty ones
#   packages   `<rice>` → the package NAMES that rice needs installed, its
#              borrow-closure included
#
# `packages` is what `strillix rice stage` checks a machine-rices rice against
# `builtin.json.packages` before staging it: a rice whose lender is not built
# in needs the lender's packages too, so the closure — not the rice's own
# record alone — is what "this rice's needs" means. Same list, same closure
# `lib/rices.nix`'s `builtIn` gives a host, so the two sides of that
# comparison are the same arithmetic.
{ ricesRoot }:
let
  lib = import ./lib;
  ricesData = import ./rices.nix { inherit lib ricesRoot; };
in
{
  manifest = ricesData.manifestAttrs;
  registry = ricesData.registryAttrs;
  packages = lib.mapAttrs (
    name: _:
    ricesData.packagesFor (
      ricesData.builtIn {
        declared = name;
        available = [ ];
      }
    )
  ) ricesData.riceDirs;
}
