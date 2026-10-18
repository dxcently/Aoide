# tests/quickshell-seam/hosts/test-two-components.nix — two component slots
# filled by their strillix providers, and nothing else from the paint set.
#
# `bar` and `dock` each import the strillix runtime pair by path, with no `strillix`
# and no `quickshell` dendrite selected: the runtime is applied once (one shell
# unit, one bridge, one home half), and both components are recorded.
_: {
  habit.dendrites.aoided.enable = true;
  habit.dendrites.aoide-packages.enable = true;

  habit.dendrites.bar = {
    enable = true;
    provider = "strillix";
  };
  habit.dendrites.dock = {
    enable = true;
    provider = "strillix";
  };

  habit.users.khoa = {
    definition = ../user.nix;
    home.enable = true;
  };

  aoide.enable = true;

  networking.hostName = "test-two-components";
  boot.loader.grub.enable = false;
  fileSystems."/" = {
    device = "none";
    fsType = "tmpfs";
  };
  system.stateVersion = "25.11";
}
