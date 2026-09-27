# tests/selection/songbook/alpha/rice.nix — a plain song, for the songbook cases.
#
# The shape the real songbook has: one module per song, self-gating on its own
# name. It configures nothing — these cases are about DISCOVERY and SELECTION,
# not about what a song paints.
{ lib, config, ... }:
{
  config = lib.mkIf (config.aoide.song == "alpha") {
    aoide.livery.palette.bg = "#000000";
  };
}
