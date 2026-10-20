# modules/dendrites/firefox.nix — the Firefox browser.
#
# Dendrite shape (CONTRACTS.md §2):
#   - Selection is the switch: no enable flag; the body runs when habit selects it.
#   - Carries its own dependencies; reads no other module.
#   - Select with one line in hosts/ (see hosts/yomi-strix/default.nix).
#
# What this dendrite does:
#   - programs.firefox via home-manager, installing the browser and letting
#     home-manager own its profile/settings management.
#   - Theming (GTK/colours) is left to the stylix lane (strillix.stylix.enable),
#     same posture as kitty.nix — no colours hard-coded here.

{ lib, ... }:
{

  config = {
    habit.home = {
      programs.firefox.enable = true;
    };
  };
}
