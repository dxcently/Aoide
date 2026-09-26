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
{
  aggregation.base.enable = true;
  aggregation.desktop.enable = true;
  aggregation.aoideos.enable = true;

  # Agents are a workstation's business, not the desktop's; a host that runs one
  # names it here.
  dendrites.claude-code.enable = true;

  users.khoa = {
    definition = ../../users/khoa.nix;
    homeManager.enable = true;
  };

  nixos =
    { lib, ... }:
    {
      imports = lib.optional (builtins.pathExists ./hardware.nix) ./hardware.nix;

      nixpkgs.hostPlatform = "x86_64-linux";

      networking.hostName = "desktop"; # ← your hostname
      networking.networkmanager.enable = true;

      time.timeZone = "UTC"; # ← your zone

      aoide.user = "khoa"; # ← your user
      # The song this host performs. REQUIRED, not decorative: `aoide.song`
      # defaults to null, and a host that names no song deploys no QML and runs
      # no shell service — the paint lanes only activate once a song is named.
      # `sonata` is the shipped standard, the guaranteed-present baseline.
      aoide.song = "sonata";
    };
}
