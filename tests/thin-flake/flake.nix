# tests/thin-flake/flake.nix — a thin flake: a consumer whose whole flake is Strillix's
# `mkFlake` over its own root.
#
# The point: this file names no host builder. `aoide.lib.mkFlake` reads this tree by
# convention — `hosts/` (a machine per directory holding a `default.nix`), `modules/default.nix`
# (a registry, merged after Strillix's own catalogue) and `rices/` (a book, merged with
# Strillix's) — and returns the machines' `nixosConfigurations` and `inventory`. Every
# `{ byClass = …; }` form in that registry is rendered by mkFlake (lib/by-class.nix), and run.sh
# checks each one by what it sets in the host it is selected by. The call also passes one of
# each of its other arguments (`registries`, `extraModules`, `overlays`), which run.sh checks.
#
# Like tests/standalone-home, run.sh evaluates this directory with `aoide` overridden to
# the ref under test, so no flake.lock is written and the tree evaluated is the ref's own.
{
  description = "A thin flake: a consumer built by Strillix's mkFlake over its own root.";

  inputs.aoide.url = "github:dxcently/Aoide";

  outputs =
    { aoide, ... }:
    aoide.lib.mkFlake {
      root = ./.;
      # A second registry source, merged after the root's: its `extra` entry is a plain module.
      registries = [
        {
          name = "extra";
          catalogue.extra = ./extra.nix;
        }
      ];
      # Modules every host takes, after the nucleus module.
      extraModules = [
        { environment.etc."thin-flake/extra-module".text = "extraModules"; }
      ];
      # Package overlays, applied after Strillix's base overlay: `pkgs.thinFlakeMarker` is `hello`.
      overlays = [ (final: prev: { thinFlakeMarker = prev.hello; }) ];
    };
}
