# tests/thin-flake/hosts/bare/default.nix — a host from this tree that selects nothing and sets no
# `strillix.hostConfig`. run.sh expects reading that option to fail, naming it.
_: {
  networking.hostName = "bare";
  nixpkgs.hostPlatform = "x86_64-linux";
  boot.loader.grub.enable = false;
  fileSystems."/" = {
    device = "none";
    fsType = "tmpfs";
  };
  system.stateVersion = "25.11";
}
