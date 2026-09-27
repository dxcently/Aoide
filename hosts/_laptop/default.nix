# hosts/_laptop/default.nix — TEMPLATE: laptop, the desktop plus power
# management. Shelved: see _desktop. To adopt, copy to `hosts/<name>/` and set
# hostName/timeZone.
{
  aggregation.base.enable = true;
  aggregation.desktop.enable = true;
  aggregation.aoideos.enable = true;

  dendrites.claude-code.enable = true;

  song.declared = "sonata";
  song.available = [ ];

  users.khoa = {
    definition = ../../users/khoa.nix;
    homeManager.enable = true;
  };

  nixos =
    { lib, ... }:
    {
      imports = lib.optional (builtins.pathExists ./hardware.nix) ./hardware.nix;

      nixpkgs.hostPlatform = "x86_64-linux";

      networking.hostName = "laptop"; # ← your hostname
      networking.networkmanager.enable = true;

      time.timeZone = "UTC"; # ← your zone

      # The one thing this shape adds to the desktop: an on-battery machine has
      # to manage its own power.
      powerManagement.enable = true;

      aoide.user = "khoa"; # ← your user
    };
}
