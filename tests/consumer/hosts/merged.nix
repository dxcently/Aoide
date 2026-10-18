# tests/consumer/hosts/merged.nix — a host whose rices is Aoide's merged with
# the consumer's own (`aoide.lib.ricesMerge`). It performs the consumer's
# `encore`, which borrows Aoide's `sonata` across the two books, and builds in
# nothing else.
_: {
  habit.aggregation.nucleus.enable = true;
  habit.aggregation.desktop.enable = true;

  habit.dendrites.compositor = {
    enable = true;
    provider = "hyprland";
  };

  habit.rice.declared = "encore";

  habit.users.fixture = {
    definition = ../user.nix;
    home.enable = true;
  };

  aoide.enable = true;
  aoide.user = "fixture";
  aoide.root = "/home/fixture/.aoide";

  nixpkgs.hostPlatform = "x86_64-linux";
  networking.hostName = "merged";
  boot.loader.grub.enable = false;
  fileSystems."/" = {
    device = "none";
    fsType = "tmpfs";
  };
  system.stateVersion = "25.11";
}
