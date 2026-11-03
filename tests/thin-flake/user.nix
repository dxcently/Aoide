# tests/thin-flake/user.nix — the thin host's one user: the account, and the home
# setting `habit.home` takes (its state version).
{ lib, ... }:
{
  users.users.thin = {
    isNormalUser = true;
    home = "/home/thin";
    createHome = true;
  };

  habit.home.home.stateVersion = lib.mkDefault "25.11";
}
