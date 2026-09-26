# tests/selection/songbook-escape/escaper/rice.nix — a song that reaches out of
# its own folder.
#
# Paired with `songEscapeClean`: the fixture songbook the other song cases use
# has no escaping `.nix`, and this one's `../` is what `checks.song-shape` must
# name. Nothing imports the file — the case reads `escapingNixFiles`, which is a
# text scan — so it never has to resolve, and it deliberately does not.
{ lib, config, ... }:
let
  song = import ../../../lib/song.nix { inherit lib; };
in
{
  config = lib.mkIf (config.aoide.song == "escaper") {
    aoide.livery.palette.bg = "#000000";
  };
}
