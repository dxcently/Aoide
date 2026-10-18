# tests/quickshell-seam/hosts/test-quickshell-bare.nix — the second strillix-less
# shape: a bare Strillix shell.
#
# Same selection as `test-quickshell-only`, minus the config: the package is
# installed and no service runs, no graphical-session anchor is claimed, and
# nothing in the evaluation is owed to strillix.
_: {
  # The services the nucleus aggregation carries, by name: the daemon's units
  # and the `aoide` binaries are what the absences below are read against.
  habit.dendrites.aoided.enable = true;
  habit.dendrites.aoide-packages.enable = true;

  habit.dendrites.quickshell.enable = true;

  habit.users.khoa = {
    definition = ../user.nix;
    home.enable = true;
  };

  aoide.enable = true;

  networking.hostName = "test-quickshell-bare";
  boot.loader.grub.enable = false;
  fileSystems."/" = {
    device = "none";
    fsType = "tmpfs";
  };
  system.stateVersion = "25.11";
}
