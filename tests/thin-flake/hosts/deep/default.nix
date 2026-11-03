# tests/thin-flake/hosts/deep/default.nix — selects `deep`, a provider-set FILE that holds
# a raw `{ byClass = …; }` marker. mkFlake does not render a marker inside a file, so this
# host's evaluation must fail, and run.sh expects the failure to name the provider.
_: {
  habit.dendrites.deep.enable = true;
  habit.dendrites.deep.provider = "one";

  networking.hostName = "deep";
  nixpkgs.hostPlatform = "x86_64-linux";
  boot.loader.grub.enable = false;
  fileSystems."/" = {
    device = "none";
    fsType = "tmpfs";
  };
  system.stateVersion = "25.11";
}
