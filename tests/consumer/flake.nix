# tests/consumer/flake.nix — a consumer flake, in the shape dxflake migrates to.
#
# The point of this file is what it does NOT contain: no path concatenated onto
# Aoide's source tree anywhere, and no threading of Aoide's own inputs.
# Everything AoideOS offers a stranger arrives through the root flake's exports
# — `nixosModules.nucleus`, `lib.{composition,livery,songbook,catalogue}`,
# `songbookRoot` and `overlays.default` — and the flake inputs Aoide's own modules need (quickshell, stylix, nvf, hyprland,
# the core itself) are closed over by `nixosModules.nucleus`, which is why this
# file declares only nixpkgs, home-manager and aoide. It does not declare
# `stylix` on purpose: selecting the `stylix` dendrite imports stylix's module from
# Aoide's own inputs, and importing a second copy through a different input
# value does not deduplicate (migration contract §12).
#
# It is NOT evaluated as a flake by nix itself: `run.sh` applies this file's
# `outputs` to the ref under test's own inputs, so a commit ref really freezes
# the tree this consumer is built against (a nested flake.lock would pin
# `aoide` to a path narHash, which proves less about a commit).
{
  description = "A stranger's flake, built only from Aoide's exported surface.";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    home-manager = {
      url = "github:nix-community/home-manager";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    aoide.url = "github:dxcently/Aoide";
  };

  outputs =
    {
      nixpkgs,
      home-manager,
      aoide,
      ...
    }:
    let
      system = "x86_64-linux";
      inherit (nixpkgs) lib;

      # The exported library values are each their file's own function, still
      # unapplied: a consumer applies them with ITS lib (the constructor runs on
      # the consumer's lib, not on Aoide's) and the songbook a host performs from.
      composition = aoide.lib.composition { inherit lib; };

      registry = {
        # `lib.catalogue` is the bulk view of the same paths
        # `aoide.nixosModules.<name>` spells one name at a time; `nucleus` is
        # not a catalogue entry and is passed below as an extra module: the
        # option contract, with the services left to the consumer's own
        # `nucleus` aggregation.
        catalogue = aoide.lib.catalogue;
        aggregations = import ./aggregations;
        overrides = { };
      };

      # One songbook per host: the consumer's own (`./songs`), or Aoide's
      # through the `songbookRoot` export. The song hook is one call: it answers
      # the registry, the selection and the platform modules for the songbook it
      # is handed, and the `users` it attaches are this consumer's own person.
      mkHost =
        name: songbookDir:
        let
          songs = aoide.lib.songbook {
            inherit lib;
            songbook = songbookDir;
          };
          song = songs.hook {
            inherit registry;
            users = [ "fixture" ];
          };
        in
        composition.mkNixosHost {
          inherit nixpkgs system;
          hostName = name;
          knownHosts = [ name ];
          inherit (song) registry selectionModules extraModulesFor;
          extraModules = [ aoide.nixosModules.nucleus ];
          host = ./hosts + "/${name}.nix";
          homeManagerModule = home-manager.nixosModules.home-manager;
          overlays = [ aoide.overlays.default ];
          specialArgs = {
            username = "fixture";
          };
        };
    in
    {
      nixosConfigurations = {
        consumer = (mkHost "consumer" ./songs).system;
        aoide-songs = (mkHost "aoide-songs" aoide.songbookRoot).system;
      };
    };
}
