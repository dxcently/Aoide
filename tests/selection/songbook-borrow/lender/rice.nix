# tests/selection/songbook-borrow/lender/rice.nix — the song whose widget bodies
# get borrowed. Minimal on purpose: the borrow cases are about OWNERSHIP, not
# about what a song paints.
{ lib, config, ... }:
{
  config = lib.mkIf (config.aoide.song == "lender") {
    aoide.livery.palette.bg = "#000000";
  };
}
