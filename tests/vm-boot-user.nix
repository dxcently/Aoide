# tests/vm-boot-user.nix — the VM's person.
#
# A user definition in the shape `users/khoa.nix` has: `nixos` creates the
# account, `homeManager` is the home floor. The VM attaches it with
# `users.khoa = { definition = ./vm-boot-user.nix; homeManager.enable = true; }`
# — the same interface a real host answers — because the constructor wires Home
# Manager only where a user asks for it, and the tree's lanes write into
# `home-manager.users.<aoide.user>`.
#
# Its own file rather than a share of `users/khoa.nix`: the test account is
# passwordless-friendly and carries no real person's groups or keys.
{
  nixos =
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
    };

  homeManager =
    { lib, ... }:
    {
      home.stateVersion = lib.mkDefault "25.11";
    };
}
