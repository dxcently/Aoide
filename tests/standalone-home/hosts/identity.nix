# tests/standalone-home/hosts/identity.nix — the standalone user's own identity: who
# and where the home is, and the one paint-adjacent setting the user keeps (the core).
{ lib, ... }:
{
  home.username = "fixture";
  home.homeDirectory = "/home/fixture";
  home.stateVersion = lib.mkDefault "25.11";
  aoide.enable = true;
}
