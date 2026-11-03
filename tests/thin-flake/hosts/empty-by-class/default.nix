# tests/thin-flake/hosts/empty-by-class/default.nix — selects `emptyByClass`, an inline record with
# no keys. run.sh expects this host's evaluation to fail, naming the entry and the reason.
_: {
  habit.dendrites.emptyByClass.enable = true;

  networking.hostName = "empty-by-class";
  nixpkgs.hostPlatform = "x86_64-linux";
  boot.loader.grub.enable = false;
  fileSystems."/" = {
    device = "none";
    fsType = "tmpfs";
  };
  system.stateVersion = "25.11";
}
