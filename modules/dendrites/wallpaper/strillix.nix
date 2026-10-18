# modules/dendrites/wallpaper/strillix.nix — the DEFAULT wallpaper provider: the
# strillix wallpaper.
#
# The shell paints everything: `AoideWallpaper.qml` draws the staged cover (the
# song's own default, or the user's pick) on the `aoide-wallpaper` layer, with
# the song's own `wallpaper` board over it when no pick applies. The layer and
# its config are the strillix runtime's, which this provider brings by path
# (`../strillix/README.md`), so it has nothing to configure and no unit to start: it
# NAMES the arrangement. The fact's value stays `"quickshell"`, the name the
# runtime, the CLI and the QML already read (README.md).
{ lib, ... }:
{
  imports = [ ../strillix/runtime.nix ];
  habit.home.imports = [ ../strillix/runtime-home.nix ];
  aoide.components.wallpaper.provider = "strillix";
  aoide.surfaces.wallpaper.owner = "quickshell";
  aoide.wallpaper.provider = lib.mkDefault "quickshell";
}
