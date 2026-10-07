# modules/dendrites/firefox.nix — the Firefox browser.
#
# Dendrite shape (CONTRACTS.md §2):
#   - Guarded on aoide.firefox.enable (default false — shipped but off).
#   - Carries its own dependencies; reads no other module.
#   - Enable with one line in hosts/ (see hosts/yomi-strix/default.nix).
#
# What this dendrite does:
#   - programs.firefox via home-manager, installing the browser and letting
#     home-manager own its profile/settings management.
#   - Theming (GTK/colours) is left to the stylix lane (aoide.stylix.enable),
#     same posture as kitty.nix — no colours hard-coded here.

{ config, lib, ... }:
{
  options.aoide.firefox.enable = lib.mkEnableOption "the Firefox browser (colours/theme deferred to the stylix lane)";

  config = lib.mkMerge [
    { aoide.firefox.enable = lib.mkDefault true; }
    (lib.mkIf config.aoide.firefox.enable {
      habit.home = {
        programs.firefox.enable = true;
      };
    })
  ];
}
