# tests/lock-greeter/songbook/aubade/rice.nix — a fixture song for tests/vm-lock-greeter.nix: a palette and nothing else (the face).
#
# A body (CONTRACTS.md §5): read only when this song is selected.
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
      aoide.livery.palette = {
        bg = "#14141e";
        fg = "#e6e6f0";
        accent = "#5c9cff";
        urgent = "#ff5c7c";
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
