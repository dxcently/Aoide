# modules/dendrites/wallpaper/vibeos.nix — the DEFAULT wallpaper provider: the
# vibeos wallpaper.
#
# The shell paints everything: `AoideWallpaper.qml` draws the staged cover (the
# song's own default, or the user's pick) on the `aoide-wallpaper` layer, with
# the song's own `wallpaper` board over it when no pick applies. The layer and
# its config are the vibeos runtime's, which this provider brings by path
# (`../lyra/README.md`), so it has nothing to configure and no unit to start: it
# NAMES the arrangement. The fact's value stays `"quickshell"`, the name the
# runtime, the CLI and the QML already read (README.md).
{ lib, ... }:
{
  imports = [ ../lyra/runtime.nix ];
  habit.home.imports = [ ../lyra/runtime-home.nix ];
  aoide.components.wallpaper.provider = "vibeos";
  aoide.surfaces.wallpaper.owner = "quickshell";
  aoide.wallpaper.provider = lib.mkDefault "quickshell";
}
