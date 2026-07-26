# modules/dendrites/btop.nix — the btop resource monitor.
#
# Dendrite shape v0 (CONTRACTS.md §2):
#   - Guarded on aoide.btop.enable (default false — shipped but off).
#   - Carries its own dependencies; reads no other module.
#   - Enable with one line in hosts/ (see hosts/common/default.nix).
#
# What this dendrite does (ported verbatim from dxflake modules/dendrites/
# btop.nix): programs.btop with a transparent background and square corners.
# The colour theme is left to the Stylix facet.
{ config, lib, ... }:
{
  options.aoide.btop.enable = lib.mkEnableOption "the btop resource monitor";

  config = lib.mkIf config.aoide.btop.enable {
    home-manager.users.${config.aoide.user} =
      { ... }:
      {
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
