# tests/consumer/songs-merge/encore/rice.nix — the consumer's own song in the
# merged songbook: it borrows Aoide's sonata, whose book is merged in beside it.
#
# The borrow reaches across books (CONTRACTS.md §5): `borrow "sonata"` resolves
# in the MERGED directory, where sonata is an ordinary song. A consumer's
# performance of an Aoide song's widgets needs nothing from Aoide's tree but the
# exported `songbookMerge` call that brought them into one directory.
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
    module =
      { song, borrow, ... }:
      {
        aoide.arrangement.widgets = (song.composeSong (borrow "sonata")).arrangement.widgets;

        aoide.livery.palette = {
          bg = "#1a1424";
          fg = "#ece6f2";
          accent = "#b48cf0";
          urgent = "#ff6b6b";
          hot = "#7ee787";
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
