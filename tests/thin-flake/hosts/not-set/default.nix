# tests/thin-flake/hosts/not-set/default.nix — selects `notSet`, a provider set whose `providers`
# is a path, not an attrset of targets. run.sh expects this host's evaluation to fail with habit's
# own message: the renderer passes the path on unrendered, and habit refuses it at selection.
_: {
  habit.dendrites.notSet.enable = true;

  networking.hostName = "not-set";
  nixpkgs.hostPlatform = "x86_64-linux";
  boot.loader.grub.enable = false;
  fileSystems."/" = {
    device = "none";
    fsType = "tmpfs";
  };
  system.stateVersion = "25.11";
}
