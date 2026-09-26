# lib/aoideos.nix — AoideOS's host constructor.
#
# Two things, and they answer to each other: `hostNames` is the machines this
# flake holds, and `mkHost name` assembles the per-host outputs for one of them.
# It is the only place that knows both halves of the flake's shape at once —
# where the module tree is, where the host records are, and which input supplies
# Home Manager — so `flake.nix` stays a list of the flake's own outputs and
# nothing else.
#
# What it does NOT do: name a host. `hostNames` is DISCOVERED — every immediate
# child directory of `hosts/` that holds a `default.nix`, minus the `_`-prefixed
# shelved ones — and it is the ONE site that reads `hosts/`. `flake.nix` builds
# its `nixosConfigurations`/`inventory` from this list and `mkHost` takes the
# same list as `knownHosts`, so "which machines exist" can never come from two
# answers that disagree.
#
# Selection happens before this file is reached, inside `mkNixosHost`
# (`lib/composition.nix`): the host record is evaluated in an ordinary
# `evalModules` pass that knows nothing about NixOS, and the platform import list
# is assembled from the result. Nothing below imports a dendrite file, and a
# capability the host did not select is never read.
#
# The songs are NOT threaded here yet. `dendrites/songbook.nix` is the lane that
# imports them, and `aggregation.aoideos.enable = true` is what selects it; S8
# makes song selection a host-record field and lands the hook that reads it
# (`selectionModules` / `extraModulesFor`) here — one place, and the reason this
# file exists apart from `flake.nix`.
{
  inputs,
  lib,
  system ? "x86_64-linux",
  username ? "khoa",
}:
let
  composition = import ./composition.nix { inherit lib; };

  # Every immediate child directory of `hosts/` holding a `default.nix`, minus
  # the `_`-prefixed shelved ones.
  hostNames = builtins.attrNames (
    lib.filterAttrs (
      name: type:
      type == "directory"
      && !lib.hasPrefix "_" name
      && builtins.pathExists (../hosts + "/${name}/default.nix")
    ) (builtins.readDir ../hosts)
  );

  # The flake's own packages (`pkgs/aoide`'s outputs and the walker's
  # `pkgs/<name>` dirs) reach a host's package set through this overlay — the
  # same source `flake.nix`'s `packages` output and the `pkg-<name>` checks
  # read, so there is one build and no second list.
  overlayModule = _: {
    nixpkgs.overlays = [
      (import ./pkgs.nix { inherit lib; }).overlay
    ];
  };
in
{
  inherit hostNames;

  mkHost =
    name:
    composition.mkNixosHost {
      inherit (inputs) nixpkgs;
      inherit system;

      hostName = name;
      # The host records this flake holds, for the one thing the constructor
      # needs the list for: an override record confined to unknown host names is
      # a typo, and it fails by name instead of applying nowhere.
      knownHosts = hostNames;

      registry = import ../modules;
      nucleus = ../modules/nucleus;
      hostModules = [ ../hosts/${name} ];
      homeManagerModule = inputs.home-manager.nixosModules.home-manager;
      extraModules = [ overlayModule ];

      # `system` is threaded to the modules rather than to `nixosSystem`: a host
      # record states its own platform (`nixpkgs.hostPlatform` in its `nixos`
      # half), because the constructor assembles the module list and lets the
      # platform come from the modules it assembled. `host` and `username` ride
      # along because modules and test fixtures have always had them.
      specialArgs = { inherit inputs username; };
    };
}
