# tests/selection/songbook-escape/escaper/rice.nix — a song that reaches out of
# its own folder: its text names `../../../lib/song.nix`, the path it would
# import instead of taking `song` as an argument.
#
# Paired with `songEscapeClean`: the fixture songbook the other song cases use
# has no escaping `.nix`, and this one's `../` is what `checks.song-shape` must
# name. Nothing imports the file — the case reads `escapingNixFiles`, which is a
# text scan — so it never has to resolve, and it deliberately does not.
{
  system.module = _: {
    aoide.livery.palette.bg = "#000000";
  };
}
