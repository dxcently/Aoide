# The declared picture is the song's own cover/<song>-desktop.<ext>, the name
# the seed and `rice stage` share.
{
  system = {
    module = { lib, ... }: {
      config = lib.mkDefault {
        aoide.theme.wallpaper = ./cover/named-desktop.jpg;
      };
    };
  };
}
