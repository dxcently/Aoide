# tests/consumer/hosts/stubbed.nix — a host that mixes value slots: the
# consumer's `bar` provider and Aoide's `dock` part, and no song.
#
# Without a song the runtime is the bare vibeos one (the seam's two-component
# host): one shell unit, the record of exactly the two slots this host filled,
# and no greeter, launcher or lock. The consumer's `stub-bar` brings no runtime,
# so the dock part is the only thing that does.
_: {
  habit.aggregation.nucleus.enable = true;
  habit.aggregation.stubbed.enable = true;

  habit.users.fixture = {
    definition = ../user.nix;
    home.enable = true;
  };

  aoide.enable = true;
  aoide.user = "fixture";
  aoide.root = "/home/fixture/.aoide";

  nixpkgs.hostPlatform = "x86_64-linux";
  networking.hostName = "stubbed";
  boot.loader.grub.enable = false;
  fileSystems."/" = {
    device = "none";
    fsType = "tmpfs";
  };
  system.stateVersion = "25.11";
}
