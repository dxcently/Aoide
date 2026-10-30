# tests/standalone-home/hosts/identity-moved.nix — the same user at another home: the
# defaults for aoide.user and the roots must follow home.username/homeDirectory.
{ lib, ... }:
{
  home.username = "guest";
  home.homeDirectory = "/srv/guest";
  home.stateVersion = lib.mkDefault "25.11";
  aoide.enable = true;
}
