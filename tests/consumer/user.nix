# tests/consumer/user.nix — the consumer's own user definition.
#
# The same shape a real user definition has: a plain module whose own settings
# create the account on the host, and whose `habit.home` is what the person's
# Home Manager configuration evaluates — through the host module's
# `home.enable`. AoideOS's own `users/khoa.nix` is not exported and is not meant
# to be: people are the machine's business.
{ lib, ... }:
{
  users.users.fixture = {
    isNormalUser = true;
    home = "/home/fixture";
    createHome = true;
  };

  habit.home.home.stateVersion = lib.mkDefault "25.11";
}
