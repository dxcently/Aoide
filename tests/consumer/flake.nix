# tests/consumer/flake.nix — a consumer flake, in the shape dxflake migrates to.
#
# The point of this file is what it does NOT contain: no path concatenated onto
# Aoide's source tree anywhere, and no threading of Aoide's own inputs. Everything AoideOS offers a stranger
# arrives through the root flake's exports — `nixosModules.nucleus`,
# `lib.{composition,livery,songbook,catalogue}`, `songbookRoot` and
# `overlays.default` — and
# the flake inputs Aoide's own lanes need (quickshell, stylix, nvf, hyprland,
# the core itself) are closed over by `nixosModules.nucleus`, which is why this
# file declares only nixpkgs, home-manager and aoide. It does not declare
# `stylix` on purpose: selecting the `stylix` lane imports stylix's module from
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
      lib = nixpkgs.lib;

      # The exported library values are each their file's own function, still
      # unapplied: a consumer applies them with ITS lib (the constructor runs on
      # the consumer's lib, not on Aoide's) and the songbook a host performs from.
      composition = aoide.lib.composition { inherit lib; };

      registry = {
        # `lib.catalogue` is the bulk view of the same paths
        # `aoide.nixosModules.<name>` spells one name at a time; `nucleus` is
        # not a catalogue entry and is passed separately below.
        catalogue = aoide.lib.catalogue;
        aggregations = import ./aggregations;
        overrides = { };
      };

      # One songbook per host: the consumer's own (`./songs`), or Aoide's
      # through the `songbookRoot` export.
      mkHost =
        name: songbookDir:
        let
          songbook = aoide.lib.songbook {
            inherit lib;
            songbook = songbookDir;
          };
        in
        composition.mkNixosHost {
          inherit nixpkgs system;
          hostName = name;
          knownHosts = [ name ];
          inherit registry;
          nucleus = aoide.nixosModules.nucleus;
          hostModules = [ (./hosts + "/${name}.nix") ];
          homeManagerModule = home-manager.nixosModules.home-manager;
          overlays = [ aoide.overlays.default ];
          # The song contract, wired exactly as §12 says a consumer wires it:
          # `selectionModule` puts the two host-record fields in the gate pass,
          # `songModules` turns the selection into `rice.nix` imports, and the
          # built-in set becomes the facts the paint lanes read.
          selectionModules = [ songbook.selectionModule ];
          extraModulesFor =
            sel:
            let
              song = songbook.check {
                inherit (sel) song;
                lyra = sel.dendrites.lyra.enable;
              };
            in
            songbook.songModules song
            ++ [
              {
                _module.args = {
                  inherit (songbook) song borrow;
                  # The directory the selection was validated against: the lyra
                  # lane reads the root from here rather than naming one.
                  songbook = songbookDir;
                };
                aoide.song = song.declared;
                aoide.songbook.builtIn = songbook.builtIn song;
              }
            ];
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
