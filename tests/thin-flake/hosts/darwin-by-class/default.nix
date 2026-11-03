# tests/thin-flake/hosts/darwin-by-class/default.nix — selects `darwinByClass`, an inline record
# whose only key is `darwin`, which no record takes. run.sh expects this host's evaluation to
# fail, naming the entry and the key.
_: {
  habit.dendrites.darwinByClass.enable = true;

  networking.hostName = "darwin-by-class";
  nixpkgs.hostPlatform = "x86_64-linux";
  boot.loader.grub.enable = false;
  fileSystems."/" = {
    device = "none";
    fsType = "tmpfs";
  };
  system.stateVersion = "25.11";
}
