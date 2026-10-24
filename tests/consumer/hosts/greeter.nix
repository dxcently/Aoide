# tests/consumer/hosts/greeter.nix — a consumer host that performs a rice with a
# Quickshell greeter face (`barcarolle`), so greetd runs cage over the greeter.
# The output is left to cage's default: the last-connected output.
_: {
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
  networking.hostName = "greeter";
  boot.loader.grub.enable = false;
  fileSystems."/" = {
    device = "none";
    fsType = "tmpfs";
  };
  system.stateVersion = "25.11";
}
