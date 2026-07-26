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

  # Committed songs self-register like dendrites: every song's rice.nix under
  # `song/repertoire/<name>/` is walked in and guards itself on
  # `aoide.song == "<name>"` (see CONTRACTS.md §5). Adding a song is a new
  # folder — never an edit to an import list. `walk` already filters non-`.nix`
  # and `/_`-shelved paths; the `.gitkeep`-only empty dir walks to `[]`, so an
  # empty repertoire is tolerated. `song/repertoire/**` is versioned score, NOT
  # a runtime dir, so reading it at eval does not violate `checks.no-song-read`
  # (that ban covers stage/ · backstage/ · auditions/ only).
  repertoire = walk ../song/repertoire;

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
    ++ repertoire
    ++ hmModule
    ++ stylixModule
    ++ [
      ../hosts/${name}
      # Inject the flake's own packages into pkgs so nucleus/facet modules can
      # reference `pkgs.aoide` / `pkgs.aoide-notes` — the SAME callPackage
      # paths the flake's `packages` output uses, so there is one source.
      (
        { ... }:
        {
          nixpkgs.overlays = [
            (final: _prev: {
              aoide = final.callPackage ../pkgs/aoide { };
              aoide-notes = final.callPackage ../pkgs/notes { };
              melete = final.callPackage ../pkgs/melete { };
              mneme = final.callPackage ../pkgs/mneme { };
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
          # Thread the flake inputs into HM submodules too, so a dendrite's
          # per-user config can import HM modules an input ships (the neovim
          # dendrite pulls inputs.nvf.homeManagerModules.default).
          home-manager.extraSpecialArgs = {
            inherit inputs;
          };
        }
      )
    ];
}
