# modules/dendrites/btop.nix — the btop resource monitor.
#
# Dendrite shape (CONTRACTS.md §2):
#   - Selection is the switch: no enable flag; the body runs when habit selects it.
#   - Carries its own dependencies; reads no other module.
#   - Selected by the nucleus aggregation (modules/aggregations/nucleus/default.nix), or one line in hosts/.
#
# What this dendrite does (ported verbatim from dxflake modules/dendrites/
# btop.nix): programs.btop with a transparent background and square corners.
# The colour theme is left to the stylix lane.

{ lib, ... }:
{

  config = {
    habit.home = _: {
      programs.btop = {
        enable = true;
        settings = {
          theme_background = false;
          rounded_corners = false;
        };
      };
    };
  };
}
