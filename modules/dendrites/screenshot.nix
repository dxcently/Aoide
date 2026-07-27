# modules/dendrites/screenshot.nix — HUMAN screenshot capture (hyprshot+satty).
#
# Dendrite shape v0 (CONTRACTS.md §2): guarded on aoide.screenshot.enable,
# carries its own dependencies, reads no other module (only aoide.user).
#
# The dxflake combo, ported verbatim: hyprshot freezes the screen and selects
# (region or output), satty annotates and copies. Binds live HERE, with the
# tools — not in the compositor facet (whose bind block documents this seam).
# HM merges the extraConfig fragment into hyprland.conf; on a box without the
# compositor facet the fragment is inert.
#
# The AGENT capture path is a separate dendrite (vision.nix — grim/slurp
# primitives, non-interactive). hyprshot depends on grim under the hood, but
# the two callers ship as two modules so either can be enabled alone.
{
  config,
  lib,
  pkgs,
  ...
}:
{
  options.aoide.screenshot.enable = lib.mkEnableOption "human screenshot capture (hyprshot + satty, dxflake combo)";

  config = lib.mkIf config.aoide.screenshot.enable {
    environment.systemPackages = with pkgs; [
      hyprshot # capture wrapper: freeze, region/window/output modes
      satty # annotation tool (dxflake combo partner)
      wl-clipboard # wl-copy — satty's copy-command target
    ];

    home-manager.users.${config.aoide.user} =
      { config, ... }:
      {
        # satty config — dxflake modules/dendrites/satty.nix, with the font
        # following the rig's monospace aesthetic instead of dxflake's Ubuntu
        # (Aoide ships no Ubuntu font).
        home.file.".config/satty/config.toml".text = ''
          [general]
          fullscreen = false
          early-exit = true
          corner-roundness = 8
          initial-tool = "brush"
          copy-command = "wl-copy"
          annotation-size-factor = 1
          output-filename = "${config.home.homeDirectory}/Pictures/Screenshots/satty-%Y%m%d-%H%M%S.png"
          save-after-copy = false
          default-hide-toolbars = false
          default-fill-shapes = false
          primary-highlighter = "block"
          actions-on-enter = ["save-to-clipboard"]
          actions-on-escape = ["exit"]

          [font]
          family = "monospace"
          style = "Regular"
        '';

        # dxflake's screenshot binds, verbatim (pkill hyprpicker clears any
        # stuck color-picker overlay before the shot).
        wayland.windowManager.hyprland.extraConfig = ''
          # ── Screenshots (screenshot dendrite) ────────────────────────────
          bind = SUPER, S, exec, pkill hyprpicker; hyprshot -z --raw -m region | satty --filename -
          bind = SUPER SHIFT, S, exec, pkill hyprpicker; hyprshot -z --raw -m output | satty --filename -
        '';
      };
  };
}
