# tests/lock-greeter/songbook/plainsong/rice.nix — a fixture song for tests/vm-lock-greeter.nix: a palette and nothing else.
#
# A body (CONTRACTS.md §5): read only when this song is selected.
{
  system = {
    providers = {
      bar = "vibeos";
      launcher = "vibeos";
      powermenu = "vibeos";
      notifications = "vibeos";
      wallpaper = "vibeos";
      dock = "vibeos";
      calendar = "vibeos";
      switcher = "vibeos";
      lock = "vibeos";
      greeter = "vibeos";
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
    members = [ "lyra" ];
    module = {
      rice.hyprland.conf = ./hyprland.conf;
    };
  };
}
