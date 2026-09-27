# tests/quickshell-seam/hosts/test-quickshell-bare.nix — the second lyra-less
# shape: a bare AoideOS shell.
#
# Same selection as `test-quickshell-only`, minus the config: the package is
# installed and no service runs, no graphical-session anchor is claimed, and
# nothing in the evaluation is owed to lyra.
{
  dendrites.quickshell.enable = true;

  users.khoa = {
    definition = ../user.nix;
    homeManager.enable = true;
  };

  nixos =
    { ... }:
    {
      aoide.enable = true;

      networking.hostName = "test-quickshell-bare";
      boot.loader.grub.enable = false;
      fileSystems."/" = {
        device = "none";
        fsType = "tmpfs";
      };
      system.stateVersion = "25.11";
    };
}
