# modules/dendrites/strillix/runtime-home.nix — the strillix runtime's home half: the
# shell lane's home half and the module `runtime.nix` publishes as
# `strillix.home`, read off the system config this home belongs to.
{ osConfig, ... }:
{
  imports = [
    ../quickshell/home.nix
    osConfig.strillix.home
  ];
}
