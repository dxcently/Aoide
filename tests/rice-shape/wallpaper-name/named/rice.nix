# The declared picture is the song's own wallpaper/<song>-desktop.<ext>, the name
# the seed and `rice stage` share.
{
  system = {
    module = { lib, ... }: {
      config = lib.mkDefault {
        aoide.theme.wallpaper = ./wallpaper/named-desktop.jpg;
      };
    };
  };
}
