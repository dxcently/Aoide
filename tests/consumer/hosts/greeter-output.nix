# tests/consumer/hosts/greeter-output.nix — the same host with `strillix.greeter.output` set, so cage
# runs extended and the greeter lands on DP-1 alone (`barcarolle`, a Quickshell greeter face).
# With the output unset, the same host keeps cage's default (tests/consumer/hosts/greeter.nix).
_: {
  strillix.greeter.output = "DP-1";

  habit.aggregation.nucleus.enable = true;
  habit.aggregation.desktop.enable = true;

  # The greeter needs a session to log into: this fixture names the compositor.
  habit.dendrites.compositor = {
    enable = true;
    provider = "hyprland";
  };

  habit.rice.declared = "barcarolle";

  habit.users.fixture = {
    definition = ../user.nix;
    home.enable = true;
  };

  aoide.enable = true;
  aoide.user = "fixture";
  aoide.root = "/home/fixture/.aoide";

  nixpkgs.hostPlatform = "x86_64-linux";
  networking.hostName = "greeter-output";
  boot.loader.grub.enable = false;
  fileSystems."/" = {
    device = "none";
    fsType = "tmpfs";
  };
  system.stateVersion = "25.11";
}
