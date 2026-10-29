# tests/standalone-home/flake.nix — a standalone Home Manager user, no NixOS.
#
# The point: Strillix's paint runtime installs on a plain `homeManagerConfiguration`
# through `homeModules.nucleus` and habit's `mkHome`. Nothing here names a system
# half: no NixOS host, no `users.users`, no polkit. What the user gets instead is
# written down in the host module (hosts/standalone.nix). The two `nixosConfigurations`
# at the bottom exist only for the one collision `homeModules.nucleus` can make with a
# NixOS host, and are read by run.sh's negative check.
#
# Like tests/consumer, this file is applied to the ref under test by the harness,
# so the tree it evaluates is the commit's own.
{
  description = "A standalone Home Manager user built from Aoide's exported surface.";

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
      pkgs = nixpkgs.legacyPackages.${system};

      composition = aoide.lib.composition { inherit lib; };

      registry = {
        catalogue = aoide.lib.catalogue;
        aggregations = import "${aoide}/modules/aggregations";
        overrides = { };
      };

      rices = aoide.lib.rices {
        inherit lib;
        inherit (aoide) ricesRoot;
      };
      rice = rices.hook { inherit registry; };

      # One constructor call per variant host. Each variant is a host file: the
      # standalone user, the same user moved to another home, a user with no
      # hostConfig, one that writes a fact, one that writes the runtime mark, one that
      # names its own user and roots, and one built without the nucleus module.
      # `host` carries the habit selections (habit's scan reads only the host file);
      # `identity`, `hostConfig` and the rest are plain modules beside it.
      mkVariant =
        {
          host,
          modules ? [ ],
          nucleus ? true,
        }:
        (composition.mkHome {
          inherit home-manager pkgs system;
          hostName = "standalone";
          knownHosts = [ "standalone" ];
          inherit (rice) registry selectionModules extraModulesFor;
          inherit host;
          extraModules = lib.optional nucleus aoide.homeModules.nucleus ++ modules;
        }).home;
      homes =
        let
          riceHost = ./hosts/rice.nix;
          identity = ./hosts/identity.nix;
          hostConfig = ./hosts/host-config.nix;
        in
        {
          standalone = mkVariant {
            host = riceHost;
            modules = [
              identity
              hostConfig
            ];
          };
          moved = mkVariant {
            host = riceHost;
            modules = [
              ./hosts/identity-moved.nix
              hostConfig
            ];
          };
          unset = mkVariant {
            host = riceHost;
            modules = [ identity ];
          };
          bad-enable = mkVariant {
            host = riceHost;
            modules = [
              identity
              hostConfig
              ./hosts/bad-enable.nix
            ];
          };
          bad-runtime = mkVariant {
            host = riceHost;
            modules = [
              identity
              hostConfig
              ./hosts/bad-runtime.nix
            ];
          };
          user-roots = mkVariant {
            host = riceHost;
            modules = [
              identity
              hostConfig
              ./hosts/user-roots.nix
            ];
          };
          no-nucleus = mkVariant {
            host = riceHost;
            nucleus = false;
            modules = [
              identity
              hostConfig
            ];
          };
        };
      # A NixOS host with home-manager: its user either imports `homeModules.nucleus`
      # (`collide`) or does not (`clean`, the control). readOnly counts every definition
      # before priority filtering, so the forwarded mkDefault and the home's plain false
      # are two definitions and `collide` fails eval naming strillix.runtime.nixos. The
      # check guards that refusal if the forward ever changes.
      nixosHost =
        userModules:
        lib.nixosSystem {
          inherit system;
          modules = [
            aoide.nixosModules.nucleus
            home-manager.nixosModules.home-manager
            {
              nixpkgs.hostPlatform = system;
              networking.hostName = "fixture";
              boot.loader.grub.enable = false;
              fileSystems."/" = {
                device = "none";
                fsType = "tmpfs";
              };
              home-manager.users.fixture = {
                imports = userModules;
                home.username = "fixture";
                home.homeDirectory = "/home/fixture";
                home.stateVersion = "25.11";
              };
            }
          ];
        };
    in
    {
      homeConfigurations = homes;
      nixosConfigurations = {
        clean = nixosHost [ ];
        collide = nixosHost [ aoide.homeModules.nucleus ];
      };
    };
}
