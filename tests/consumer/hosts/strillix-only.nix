# tests/consumer/hosts/strillix-only.nix — Strillix with Aoide off.
#
# The desktop aggregation alone: the renderer, its ten components and the theme.
# No `nucleus` aggregation, so no `aoided` member, and `aoide.enable` is left at
# its default, off. The two scalars below name the user and the root the desktop's
# units read; they do not turn the core on.
_: {
  habit.aggregation.desktop.enable = true;

  # The greeter needs a session to log into: this fixture names the compositor.
  habit.dendrites.compositor = {
    enable = true;
    provider = "hyprland";
  };

  habit.users.fixture = {
    definition = ../user.nix;
    home.enable = true;
  };

  aoide.user = "fixture";
  aoide.root = "/home/fixture/.aoide";

  nixpkgs.hostPlatform = "x86_64-linux";
  networking.hostName = "strillix-only";
  boot.loader.grub.enable = false;
  fileSystems."/" = {
    device = "none";
    fsType = "tmpfs";
  };
  system.stateVersion = "25.11";
}
