# modules/dendrites/wallpaper/strillix.nix — the DEFAULT wallpaper provider: the
# strillix wallpaper.
#
# The shell paints everything: `AoideWallpaper.qml` draws the staged wallpaper (the
# rice's own default, or the user's pick) on the `aoide-wallpaper` layer, with
# the rice's own `wallpaper` board over it when no pick applies. The layer and
# its config are the strillix runtime's, which this provider brings by path
# (`../strillix/README.md`), so it has nothing to configure and no unit to start: it
# NAMES the layout. The fact's value stays `"quickshell"`, the name the
# runtime, the CLI and the QML already read (README.md).
{ lib, ... }:
let
  claim = {
    components.wallpaper.provider = "strillix";
    surfaces.wallpaper.owner = "quickshell";
    wallpaper.provider = lib.mkDefault "quickshell";
  };
in
{
  imports = [ ../strillix/runtime.nix ];
  habit.home.strillix = claim;
  habit.home.imports = [ ../strillix/runtime-home.nix ];
  strillix = claim;
}
