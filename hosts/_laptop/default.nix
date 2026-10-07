# hosts/_laptop/default.nix — TEMPLATE: laptop, the desktop plus power
# management. Shelved: see _desktop. To adopt, copy to `hosts/<name>/` and set
# hostName/timeZone.
{ lib, ... }:
{
  habit.aggregation.base.enable = true;
  habit.aggregation.desktop.enable = true;
  habit.aggregation.aoideos.enable = true;

  habit.dendrites.claude-code.enable = true;

  habit.song.declared = "sonata";
  habit.song.available = [ ];

  habit.users.khoa = {
    definition = ../../users/khoa.nix;
    home.enable = true;
  };

  imports = lib.optional (builtins.pathExists ./hardware.nix) ./hardware.nix;

  nixpkgs.hostPlatform = "x86_64-linux";

  networking.hostName = "laptop"; # ← your hostname
  networking.networkmanager.enable = true;

  time.timeZone = "UTC"; # ← your zone

  # The one thing this shape adds to the desktop: an on-battery machine has
  # to manage its own power.
  powerManagement.enable = true;

  aoide.user = "khoa"; # ← your user
}
