# modules/dendrites/nh.nix — nh (nix-helper) rebuild wrapper.
#
# Dendrite shape v0 (CONTRACTS.md §2):
#   - Guarded on aoide.nh.enable (default false — shipped but off).
#   - Carries its own dependencies; reads no other module.
#   - Enable with one line in hosts/ (see hosts/common/default.nix).
#
# What this dendrite does:
#   - Turns on programs.nh for the aoide user (home-manager), pointed at the
#     Aoide flake so `nh os switch` and friends default to this checkout.
#   - Enables nh's generation GC (`nh clean`) with the dxflake retention policy.
#
# Ported from dxflake modules/dendrites/nh.nix. The flake path is rehomed from
# /home/khoa/dxflake/ to /home/khoa/Aoide/ (the `ad*` alias family in bash.nix
# drives the same target).
{ config, lib, ... }:
let
  flakeDir = "/home/khoa/Aoide/";
in
{
  options.aoide.nh.enable = lib.mkEnableOption "nh (nix-helper) rebuild wrapper pointed at the Aoide flake";

  config = lib.mkIf config.aoide.nh.enable {
    home-manager.users.${config.aoide.user} = _: {
      programs.nh = {
        enable = true;
        clean = {
          enable = true;
          extraArgs = "--keep-since 1w --keep 10";
        };
        flake = flakeDir;
      };
    };
  };
}
