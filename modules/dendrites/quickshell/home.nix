# modules/dendrites/quickshell/home.nix — the shell lane's home half: the
# module `system.nix` publishes as `aoide.quickshell.home`, read off the
# system config this home belongs to.
{ osConfig, ... }:
{
  imports = [ osConfig.aoide.quickshell.home ];
}
