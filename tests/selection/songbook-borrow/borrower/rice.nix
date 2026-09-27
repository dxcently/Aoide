# tests/selection/songbook-borrow/borrower/rice.nix — the song that borrows.
{ lib, config, ... }:
{
  config = lib.mkIf (config.aoide.song == "borrower") {
    aoide.livery.palette.bg = "#000000";
  };
}
