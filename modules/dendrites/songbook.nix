# modules/dendrites/songbook.nix — the committed songs.
#
# INTERIM, and it says so out loud: this lane imports EVERY song
# `song/songbook/` discovers, on any host that selects it, and the host names the
# one it performs with `aoide.song`. S8 replaces this with per-host selection —
# `song.declared` / `song.available` on the host record, the selected songs'
# `rice.nix` files imported and nothing else — at which point this lane stops
# being "all discovered" and becomes the thing that reads the selection. Until
# then, "all discovered" is exactly what `lib/mkHost.nix` did before it was
# deleted, which is what keeps this slice a refactor.
#
# There is no `aoide.songbook.enable` here, deliberately. A lane's own flag is
# for a capability a host might want to drop while keeping its neighbours; the
# songbook is not droppable independently of the desktop that performs it, so
# selection IS the switch — the `aoideos` aggregation names this member, and a
# host that wants no song imported selects no `aoideos` (see _server).
#
# The songs ride the `body`, not the lane, so the whole-tree aggregate
# (`modules/dendrites/default.nix`) carries them too: taking the tree means
# taking everything, songs included, exactly as it did before this slice.
let
  body =
    { lib, ... }:
    {
      imports = (import ../../lib/walk.nix { inherit lib; }) ../../song/songbook;
    };
in
{
  inherit body;

  nixos =
    { ... }:
    {
      imports = [ body ];
    };
}
