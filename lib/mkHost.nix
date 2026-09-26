# lib/mkHost.nix — assemble one host's nixosSystem.
#
# A host is: the module tree (each of modules/{dendrites,nucleus} names its own
# files; modules/default.nix is the registry the constructor reads, not a
# module) + every committed song (still discovered by lib/walk.nix) + the host's
# own dir + home-manager. Each dendrite carries its own dependencies — the
# stylix lane imports the Stylix module itself — so this only wires the fixed
# inputs and passes `specialArgs` every module can rely on.
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

  # Committed songs self-register like dendrites: every song's rice.nix under
  # `song/songbook/<name>/` is walked in and guards itself on
  # `aoide.song == "<name>"` (see CONTRACTS.md §5). Adding a song is a new
  # folder — never an edit to an import list. `walk` already filters non-`.nix`
  # and `/_`-shelved paths; the `.gitkeep`-only empty dir walks to `[]`, so an
  # empty songbook is tolerated. `song/songbook/**` is versioned score, NOT
  # a runtime dir, so reading it at eval does not violate `checks.no-song-read`
  # (that ban covers stage/ · auditions/ only).
  songbook = walk ../song/songbook;

  # home-manager and stylix ride as NixOS modules when their inputs are present.
  # Kept tolerant: if an input is absent (minimal eval), we simply omit it so
  # `nix flake check` still evaluates.
  #
  # The stylix module is normally the stylix LANE's own dependency
  # (`modules/dendrites/stylix.nix`'s `nixos` half), which is what the
  # constructor imports. This whole-tree path imports bodies instead, so until
  # it goes (S7) the module has to be added here as well — one copy per path,
  # never both at once.
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
  modules = [
    # The two aggregates, in the order the module system merges them:
    # dendrites (opt-in), nucleus (unconditional core).
    # `modules/default.nix` is the registry — data the constructor reads — so a
    # host imports the directories directly.
    ../modules/dendrites
    ../modules/nucleus
  ]
  ++ songbook
  ++ hmModule
  ++ stylixModule
  ++ [
    ../hosts/${name}
    # Inject the flake's own packages into pkgs so nucleus/lane modules can
    # reference `pkgs.aoide` / … — auto-discovered by
    # lib/pkgs.nix from the SAME pkgs/<name> dirs the flake's `packages`
    # output uses, so there is one source. The overlay form also guards each
    # name against shadowing a stock nixpkgs attribute. `aoide` itself is
    # self-flaked (pkgs/aoide/flake.nix) and skipped by the walker — it
    # arrives via `inputs.aoide.nixosModules.default`, imported by
    # `modules/nucleus/options.nix` and carrying `overlays.default` with it,
    # the same derivation the flake's `packages.aoide` and `pkg-aoide` use.
    (_: {
      nixpkgs.overlays = [
        (import ./pkgs.nix { inherit lib; }).overlay
      ];
    })
    # home-manager house defaults, applied only when the module is present.
    (
      _:
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
