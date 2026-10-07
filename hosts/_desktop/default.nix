# hosts/_desktop/default.nix — TEMPLATE: desktop workstation.
#
# Shelved (the `_` prefix): `flake.nix` discovers `hosts/` minus the `_`-prefixed
# entries, so nothing builds or evaluates this file. yomi-strix stays the living
# reference; this is the generic starting shape. To adopt:
#   1. cp -r hosts/_desktop hosts/<your-hostname>
#   2. set hostName below; drop a committed hardware.nix beside this file
#      (the guarded import tolerates its absence)
#   3. nixos-rebuild switch --flake .#<your-hostname>
#
# No list anywhere needs the new name — hosts are discovered.
{ lib, ... }:
{
  habit.aggregation.base.enable = true;
  habit.aggregation.desktop.enable = true;
  habit.aggregation.aoideos.enable = true;

  # Agents are a workstation's business, not the desktop's; a host that runs one
  # names it here.
  habit.dendrites.claude-code.enable = true;

  # The song this host performs, and what it additionally builds in. `sonata` is
  # the shipped standard, the guaranteed-present baseline. Any committed rice is
  # reachable from the machine's own songbook without a rebuild
  # (`aoide rice stage <n>`) — `declared` is only what is painted at boot.
  habit.song.declared = "sonata";
  habit.song.available = [ ];

  habit.users.khoa = {
    definition = ../../users/khoa.nix;
    home.enable = true;
  };

  imports = lib.optional (builtins.pathExists ./hardware.nix) ./hardware.nix;

  nixpkgs.hostPlatform = "x86_64-linux";

  networking.hostName = "desktop"; # ← your hostname
  networking.networkmanager.enable = true;

  time.timeZone = "UTC"; # ← your zone

  aoide.user = "khoa"; # ← your user
}
