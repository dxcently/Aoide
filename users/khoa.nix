# users/khoa.nix — the person, shared across machines.
#
# One definition, many hosts: a machine attaches this file
# (`habit.users.khoa.definition = ../../users/khoa.nix;`) and never copies it.
# What is here is what is true of khoa on EVERY host — the account, and the home
# floor every machine's Home Manager configuration starts from. Per-machine
# extras belong to the machine (`habit.users.khoa.home.config` in that host's
# module).
#
# A user definition is a plain module. Its own settings create the account on a
# machine that attaches it; its `habit.home` is the home floor.
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

  # The home floor: Home Manager's compat pin, and nothing else. Packages and
  # programs are the machine's business or a selected module's. mkDefault
  # because it IS a floor: a machine that needs a different pin states it in its
  # own module (`habit.users.khoa.home.config`) and wins, which is what
  # `users/README.md` promises.
  habit.home =
    { lib, ... }:
    {
      home.stateVersion = lib.mkDefault "25.11";
    };
}
