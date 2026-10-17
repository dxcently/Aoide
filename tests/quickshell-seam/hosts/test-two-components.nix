# tests/quickshell-seam/hosts/test-two-components.nix — two component slots
# filled by their vibeos providers, and nothing else from the paint set.
#
# `bar` and `dock` each import the vibeos runtime pair by path, with no `lyra`
# and no `quickshell` dendrite selected: the runtime is applied once (one shell
# unit, one bridge, one home half), and both components are recorded.
_: {
  habit.dendrites.aoided.enable = true;
  habit.dendrites.aoide-packages.enable = true;

  habit.dendrites.bar = {
    enable = true;
    provider = "vibeos";
  };
  habit.dendrites.dock = {
    enable = true;
    provider = "vibeos";
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
