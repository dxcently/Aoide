# modules/dendrites/nh.nix — nh (nix-helper) rebuild wrapper.
#
# Dendrite shape (CONTRACTS.md §2):
#   - Guarded on aoide.nh.enable (default false — shipped but off).
#   - Carries its own dependencies; reads no other module.
#   - Enable with one line in hosts/ (the `base` aggregation defaults it on —
#     see modules/aggregations/base/default.nix).
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

  config = lib.mkMerge [
    { aoide.nh.enable = lib.mkDefault true; }
    (lib.mkIf config.aoide.nh.enable {
      habit.home = _: {
        programs.nh = {
          enable = true;
          clean = {
            enable = true;
            extraArgs = "--keep-since 1w --keep 10";
          };
          flake = flakeDir;
        };
      };
    })
  ];
}
