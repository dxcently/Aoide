# A picture under any other name: the seed would publish it, a stage would not
# find it, so the lint refuses the song.
{
  system = {
    module = { lib, ... }: {
      config = lib.mkDefault {
        aoide.theme.wallpaper = ./wallpaper/dusk.jpg;
      };
    };
  };
}
