# tests/consumer/songs/solo/rice.nix — the fixture's declared song.
#
# A consumer's song is a song: it sets ONLY `aoide.livery` (CONTRACTS §5), it
# self-gates on `aoide.song`, and it reaches its own widget records through the
# injected `borrow` door rather than through a path.
{
  lib,
  config,
  song,
  borrow,
  ...
}:
let
  widgets = borrow "solo";
in
{
  config = lib.mkIf (config.aoide.song == "solo") {
    aoide.arrangement.widgets = (song.composeSong widgets).arrangement.widgets;

    aoide.livery.palette = {
      bg = "#14161c";
      fg = "#e6e8ef";
      accent = "#8ab4f8";
      urgent = "#ff6b6b";
      hot = "#7ee787";
    };
  };
}
