# tests/selection/songbook/_shelved/rice.nix — shelved, like a `_`-prefixed
# dendrite: discovered by nothing, selectable by nothing.
{ lib, config, ... }:
{
  config = lib.mkIf (config.aoide.song == "_shelved") {
    aoide.livery.palette.bg = "#000000";
  };
}
