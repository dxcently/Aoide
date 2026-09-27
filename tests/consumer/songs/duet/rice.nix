# tests/consumer/songs/duet/rice.nix — the song built in but NOT performed.
#
# `song.available = [ "duet" ]` imports this file (it is built in: its widgets
# land in the deployed tree and its folder in the shipped templates), and its
# own gate — `aoide.song == "duet"`, which nothing sets on this host — keeps it
# from dressing a desktop it is not painting. An available song that quietly
# applied its livery would be the bug this file's shape disproves.
{
  lib,
  config,
  ...
}:
{
  config = lib.mkIf (config.aoide.song == "duet") {
    aoide.livery.palette = {
      bg = "#1c1418";
      fg = "#efe6e8";
      accent = "#f88ab4";
      urgent = "#ff6b6b";
      hot = "#7ee787";
    };
  };
}
