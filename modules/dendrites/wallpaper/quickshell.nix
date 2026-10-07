# modules/dendrites/wallpaper/quickshell.nix — the DEFAULT wallpaper provider.
#
# The shell paints everything: `AoideWallpaper.qml` draws the staged cover (the
# song's own default, or the user's pick) on the `aoide-wallpaper` layer, with
# the song's own `wallpaper` board over it when no pick applies — the layer and
# its config are the `lyra` lane's, so this provider has nothing to configure and
# no unit to start: it NAMES the arrangement.

{ lib, ... }:
{
  aoide.wallpaper.provider = lib.mkDefault "quickshell";
}
