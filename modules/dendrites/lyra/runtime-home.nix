# modules/dendrites/lyra/runtime-home.nix — the vibeos runtime's home half: the
# shell lane's home half and the module `runtime.nix` publishes as
# `aoide.lyra.home`, read off the system config this home belongs to.
{ osConfig, ... }:
{
  imports = [
    ../quickshell/home.nix
    osConfig.aoide.lyra.home
  ];
}
