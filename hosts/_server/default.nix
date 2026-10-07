# hosts/_server/default.nix — TEMPLATE: a headless box. Shelved: see _desktop.
#
# The same interface answered differently: the floor and nothing else, no
# graphical session, and therefore no songs — `songbook` rides `aoideos`, and a
# host that selects no `aoideos` imports no committed song at all. A headless
# machine has no terminal to open, so `kitty` is the one floor member it drops;
# `enable = false` outranks the aggregation's membership.
{ lib, ... }:
{
  habit.aggregation.base.enable = true;

  habit.dendrites.kitty.enable = false;

  habit.users.khoa = {
    definition = ../../users/khoa.nix;
    home.enable = true;
  };

  imports = lib.optional (builtins.pathExists ./hardware.nix) ./hardware.nix;

  nixpkgs.hostPlatform = "x86_64-linux";

  networking.hostName = "server"; # ← your hostname

  time.timeZone = "UTC";

  aoide.user = "khoa"; # ← your user
}
