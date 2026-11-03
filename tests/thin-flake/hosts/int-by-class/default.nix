# tests/thin-flake/hosts/int-by-class/default.nix — selects `intByClass`, an inline record that is
# not an attrset. run.sh expects this host's evaluation to fail, naming the entry and the type.
_: {
  habit.dendrites.intByClass.enable = true;

  networking.hostName = "int-by-class";
  nixpkgs.hostPlatform = "x86_64-linux";
  boot.loader.grub.enable = false;
  fileSystems."/" = {
    device = "none";
    fsType = "tmpfs";
  };
  system.stateVersion = "25.11";
}
