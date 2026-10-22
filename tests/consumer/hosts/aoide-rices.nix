# tests/consumer/hosts/aoide-rices.nix — a consumer host that performs one of
# Strillix's OWN rices.
#
# The same record shape as `consumer.nix`; what differs is the rices the
# flake hands its constructor: Aoide's, through the `ricesRoot` export, never
# a path built into Aoide's tree. `sonata` borrows its own `_widgets/` shelf, so
# the borrow door is exercised over Aoide's rices too.
_: {
  habit.aggregation.nucleus.enable = true;
  habit.aggregation.desktop.enable = true;

  # The rices bring the greeter, and the greeter needs a session to log into:
  # this fixture names the compositor that is that session.
  habit.dendrites.compositor = {
    enable = true;
    provider = "hyprland";
  };

  habit.rice.declared = "sonata";

  # A venue's own theme value, written the way a real host writes one (a
  # host-wide `blurEnabled`, a `terminalOpacity`). It must outrank only itself,
  # never the performed rice's theme (CONTRACTS.md §5).
  strillix.theme.geometry.gapsOut = 3;

  habit.users.fixture = {
    definition = ../user.nix;
    home.enable = true;
  };

  aoide.enable = true;
  aoide.user = "fixture";
  aoide.root = "/home/fixture/.aoide";

  nixpkgs.hostPlatform = "x86_64-linux";
  networking.hostName = "aoide-rices";
  boot.loader.grub.enable = false;
  fileSystems."/" = {
    device = "none";
    fsType = "tmpfs";
  };
  system.stateVersion = "25.11";
}
