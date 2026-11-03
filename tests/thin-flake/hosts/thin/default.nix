# tests/thin-flake/hosts/thin/default.nix — the thin host: a minimal headless NixOS
# machine. It selects the fixture's dendrites and no Strillix one. Its one user takes
# Home Manager, so the Home Manager modules reach it, and each check in run.sh reads what
# one of the selected dendrites set.
_: {
  habit.dendrites.both.enable = true;
  habit.dendrites.extra.enable = true;
  habit.dendrites.inline.enable = true;
  habit.dendrites.sysonly.enable = true;
  habit.dendrites.picked.enable = true;
  habit.dendrites.picked.provider = "one";

  habit.users.thin = {
    definition = ../../user.nix;
    home.enable = true;
  };

  networking.hostName = "thin";
  strillix.hostConfig = "/srv/thin-flake/hosts/thin/default.nix";
  nixpkgs.hostPlatform = "x86_64-linux";
  boot.loader.grub.enable = false;
  fileSystems."/" = {
    device = "none";
    fsType = "tmpfs";
  };
  system.stateVersion = "25.11";
}
