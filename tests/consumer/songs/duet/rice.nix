# tests/consumer/songs/duet/rice.nix — the song built in but NOT performed.
#
# `habit.rice.available = [ "duet" ]` builds it in (its widgets land in the deployed
# tree and its folder in the shipped templates), but a body is read only for a
# DECLARED song: its `system` half is never imported here, so its theme cannot
# dress a desktop it is not painting. An available song that quietly applied its
# theme would be the bug this file's shape disproves.
{
  system = {
    providers = {
      bar = "strillix";
      launcher = "strillix";
      powermenu = "strillix";
      notifications = "strillix";
      wallpaper = "strillix";
      dock = "strillix";
      calendar = "strillix";
      switcher = "strillix";
      lock = "strillix";
      greeter = "strillix";
    };
    module = _: {
      aoide.theme.palette = {
        bg = "#1c1418";
        fg = "#efe6e8";
        accent = "#f88ab4";
        urgent = "#ff6b6b";
        hot = "#7ee787";
      };
    };
  };
  home = {
    members = [ "strillix" ];
    module = {
      rice.hyprland.conf = ./hyprland.conf;
    };
  };
}
