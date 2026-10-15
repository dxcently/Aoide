# modules/dendrites/nh.nix — nh (nix-helper) rebuild wrapper.
#
# Dendrite shape (CONTRACTS.md §2):
#   - Selection is the switch: no enable flag; the body runs when habit selects it.
#   - Carries its own dependencies; reads no other module.
#   - Selected by the nucleus aggregation (modules/aggregations/nucleus/default.nix), or one line in hosts/.
#
# What this dendrite does:
#   - Turns on programs.nh for the aoide user (home-manager), pointed at the
#     Aoide flake so `nh os switch` and friends default to this checkout.
#   - Enables nh's generation GC (`nh clean`) with the dxflake retention policy.
#
# Ported from dxflake modules/dendrites/nh.nix. The flake path is rehomed from
# /home/khoa/dxflake/ to /home/khoa/Aoide/ (the `ad*` alias family in bash.nix
# drives the same target).

{ lib, ... }:
let
  flakeDir = "/home/khoa/Aoide/";
in
{

  config = {
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
  };
}
