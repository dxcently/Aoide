# tests/thin-flake/hosts/bad/default.nix — selects the one entry whose file is a plain
# module, not a byClass record. run.sh expects this host's evaluation to fail, by name.
_: {
  habit.dendrites.bad.enable = true;

  networking.hostName = "bad";
  nixpkgs.hostPlatform = "x86_64-linux";
  boot.loader.grub.enable = false;
  fileSystems."/" = {
    device = "none";
    fsType = "tmpfs";
  };
  system.stateVersion = "25.11";
}
