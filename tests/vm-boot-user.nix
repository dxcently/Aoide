# tests/vm-boot-user.nix — the VM's person.
#
# A user definition in the shape `users/khoa.nix` has: a plain module whose own
# settings create the account and whose `habit.home` is the home floor. The VM
# attaches it with `habit.users.khoa = { definition = ./vm-boot-user.nix;
# home.enable = true; }` — the same interface a real host answers — because the
# constructor wires Home Manager only where a user asks for it, and the tree's
# dendrites write into that user's home.
#
# Its own file rather than a share of `users/khoa.nix`: the test account is
# passwordless-friendly and carries no real person's groups or keys.
{ lib, ... }:
{
  users.users.khoa = {
    isNormalUser = lib.mkDefault true;
    extraGroups = lib.mkDefault [
      "wheel"
      "video"
      "audio"
      "networkmanager"
    ];
    # Passwordless-friendly for test invocations.
    initialPassword = "test";
  };

  habit.home =
    { lib, ... }:
    {
      home.stateVersion = lib.mkDefault "25.11";
    };
}
