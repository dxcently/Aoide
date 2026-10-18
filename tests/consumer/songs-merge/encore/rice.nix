# tests/consumer/songs-merge/encore/rice.nix — the consumer's own song in the
# merged rices: it borrows Aoide's sonata, whose book is merged in beside it.
#
# The borrow reaches across books (CONTRACTS.md §5): `borrow "sonata"` resolves
# in the MERGED directory, where sonata is an ordinary song. A consumer's
# performance of an Aoide song's widgets needs nothing from Aoide's tree but the
# exported `ricesMerge` call that brought them into one directory.
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
      { song, borrow, ... }:
      {
        aoide.layout.widgets = (song.composeSong (borrow "sonata")).layout.widgets;

        aoide.theme.palette = {
          bg = "#1a1424";
          fg = "#ece6f2";
          accent = "#b48cf0";
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
