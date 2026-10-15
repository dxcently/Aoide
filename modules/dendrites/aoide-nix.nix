# modules/dendrites/aoide-nix.nix — nix itself, configured for a flake-native system.
#
# Aoide IS a flake: every rebuild, check, and `aoide update` runs through the
# flake CLI. The `nucleus` aggregation therefore turns the flake feature set on system-wide.
# Found live on first switch: dxflake's nucleus had carried these settings and
# the "essentials only" port dropped them, leaving a box whose own framework
# could not evaluate itself.
{
  config = {
    aoide.enable = true;
    nix.settings.experimental-features = [
      "nix-command"
      "flakes"
    ];
  };
}
