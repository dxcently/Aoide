# tests/consumer/user.nix — the consumer's own user definition.
#
# The same two halves a real user definition has: the account on the host
# (`nixos`), and — through the host record's `homeManager.enable` — the lanes
# this person evaluates. AoideOS's own `users/khoa.nix` is not exported and is
# not meant to be: people are the machine's business.
{
  nixos =
    { ... }:
    {
      users.users.fixture = {
        isNormalUser = true;
        home = "/home/fixture";
        createHome = true;
      };
    };

  homeManager =
    { lib, ... }:
    {
      home.stateVersion = lib.mkDefault "25.11";
    };
}
