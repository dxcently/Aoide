# tests/consumer/rices/solo/rice.nix — the fixture's declared rice, as a body.
#
# A consumer's rice is a rice: its `system` half selects the paint providers and
# sets ONLY `strillix.theme` and `strillix.layout` (CONTRACTS.md §5). It reaches its
# own widget records through the injected `borrow` door rather than through a path.
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
    module =
      { rice, borrow, ... }:
      let
        widgets = borrow "solo";
      in
      {
        strillix.layout.widgets = (rice.composeRice widgets).layout.widgets;

        strillix.theme.palette = {
          bg = "#14161c";
          fg = "#e6e8ef";
          accent = "#8ab4f8";
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
