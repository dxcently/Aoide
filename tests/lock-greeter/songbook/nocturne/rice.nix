# A fixture song for tests/vm-lock-greeter.nix: a greeter face that never reports ready.
{ lib, config, ... }:
{
  config = lib.mkIf (config.aoide.song == "nocturne") {
    aoide.livery.palette = {
      bg = "#14141e";
      fg = "#e6e6f0";
      accent = "#5c9cff";
      urgent = "#ff5c7c";
    };
  };
}
