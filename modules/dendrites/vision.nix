# modules/dendrites/vision.nix — AGENT screen capture primitives (grim/slurp).
#
# Dendrite shape v0 (CONTRACTS.md §2): guarded on aoide.vision.enable,
# carries its own dependencies, reads no other module.
#
# "Vision" for agents working the rice: grim captures a Wayland screenshot
# non-interactively — no freeze overlay, no notification, no picker — which
# is what an agent needs to LOOK at what it just changed:
#
#   grim shot.png                    # full screen
#   grim -g "0,0 1920x28" bar.png    # exact region (geometry from
#                                    #   hyprctl layers / hyprctl clients)
#
# slurp rides along as the region-select primitive for semi-attended flows
# (a human drags a region, the agent consumes the geometry):
#
#   grim -g "$(slurp)" pick.png
#
# The HUMAN capture path is a separate dendrite (screenshot.nix — hyprshot +
# satty + binds). hyprshot wraps grim internally, but the two callers ship as
# two modules so either can be enabled alone.
{
  config,
  lib,
  pkgs,
  ...
}:
{
  options.aoide.vision.enable = lib.mkEnableOption "agent screen-capture primitives (grim + slurp on PATH)";

  config = lib.mkIf config.aoide.vision.enable {
    environment.systemPackages = with pkgs; [
      grim # non-interactive capture: full screen or exact geometry
      slurp # region-select primitive (emits geometry on stdout)
    ];
  };
}
