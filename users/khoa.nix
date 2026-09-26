# users/khoa.nix — the person, shared across machines.
#
# One definition, many hosts: a machine attaches this file
# (`users.khoa.definition = ../../users/khoa.nix;`) and never copies it. What is
# here is what is true of khoa on EVERY host — the account, and the home floor
# every machine's Home Manager lane starts from. Per-machine extras belong to the
# machine (`users.khoa.homeManager.config` in that host's record).
#
# A user definition holds two halves and nothing else: `nixos` creates the
# account on a machine that attaches it, `homeManager` is the home floor. The
# constructor says so by name if a definition exposes no `nixos` half — an
# account cannot be created from a home floor alone.
{
  nixos =
    { lib, ... }:
    {
      users.users.khoa = {
        isNormalUser = lib.mkDefault true;
        # mkDefault, as these were while they lived in `hosts/common`: a machine
        # that needs another group restates the list and wins.
        extraGroups = lib.mkDefault [
          "wheel"
          "video"
          "audio"
          "networkmanager"
        ];
      };
    };

  # The home floor: Home Manager's compat pin, and nothing else. Packages and
  # programs are the machine's business or a selected lane's. mkDefault because
  # it IS a floor: a machine that needs a different pin states it in its own
  # record (`users.khoa.homeManager.config`) and wins, which is what
  # `users/README.md` promises.
  homeManager =
    { lib, ... }:
    {
      home.stateVersion = lib.mkDefault "25.11";
    };
}
