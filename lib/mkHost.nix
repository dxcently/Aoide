# lib/mkHost.nix — assemble one host's nixosSystem.
#
# A host is: the whole walked module tree (nucleus + dendrites + facets + rime,
# discovered by lib/walk.nix) + the host's own dir + home-manager + stylix.
# The walker does the discovery; this only wires the fixed inputs and passes
# `specialArgs` every module can rely on.
#
# `hosts/` knows dendrites; dendrites never know hosts (dxflake separation,
# verbatim). A host `default.nix` only flips `aoide.*` flags and imports its
# guarded ./hardware.nix — it never imports module files directly.
{
  inputs,
  lib,
  system ? "x86_64-linux",
  username ? "khoa",
}:
name:
let
  walk = import ./walk.nix { inherit lib; };
  # Modules that walk from `../modules` — the whole snowflake, self-registered.
  discovered = walk ../modules;

  # home-manager and stylix ride as NixOS modules when their inputs are present.
  # Kept tolerant: if an input is absent (minimal eval), we simply omit it so
  # `nix flake check` still evaluates.
  optionalModule = attr: path: lib.optional (inputs ? ${attr}) path;
  hmModule = optionalModule "home-manager" (inputs.home-manager.nixosModules.home-manager or { });
  stylixModule = optionalModule "stylix" (inputs.stylix.nixosModules.stylix or { });
in
inputs.nixpkgs.lib.nixosSystem {
  inherit system;
  specialArgs = {
    host = name;
    inherit
      inputs
      username
      system
      ;
  };
  modules =
    discovered
    ++ hmModule
    ++ stylixModule
    ++ [
      ../hosts/${name}
      # Inject the flake's own packages into pkgs so nucleus/facet modules can
      # reference `pkgs.aoide` / `pkgs.aoide-tokens` — the SAME callPackage
      # paths the flake's `packages` output uses, so there is one source.
      (
        { ... }:
        {
          nixpkgs.overlays = [
            (final: _prev: {
              aoide = final.callPackage ../pkgs/aoide { };
              aoide-tokens = final.callPackage ../pkgs/tokens { };
            })
          ];
        }
      )
      # home-manager house defaults, applied only when the module is present.
      (
        { ... }:
        lib.optionalAttrs (inputs ? home-manager) {
          home-manager.useGlobalPkgs = lib.mkDefault true;
          home-manager.useUserPackages = lib.mkDefault true;
        }
      )
    ];
}
