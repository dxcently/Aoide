# The declared picture is the rice's own wallpaper/<rice>-desktop.<ext>, the name
# the seed and `rice stage` share.
{
  system = {
    module = { lib, ... }: {
      config = lib.mkDefault {
        strillix.theme.wallpaper = ./wallpaper/named-desktop.jpg;
      };
    };
  };
}
