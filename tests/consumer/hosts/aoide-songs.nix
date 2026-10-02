# tests/consumer/hosts/aoide-songs.nix — a consumer host that performs one of
# Aoide's OWN songs.
#
# The same record shape as `consumer.nix`; what differs is the songbook the
# flake hands its constructor: Aoide's, through the `songbookRoot` export, never
# a path built into Aoide's tree. `sonata` borrows its own `_widgets/` shelf, so
# the borrow door is exercised over Aoide's songbook too.
{
  aggregation.desktop.enable = true;

  song.declared = "sonata";

  users.fixture = {
    definition = ../user.nix;
    homeManager.enable = true;
  };

  nixos =
    { ... }:
    {
      aoide.enable = true;
      aoide.user = "fixture";
      aoide.root = "/home/fixture/.aoide";

      nixpkgs.hostPlatform = "x86_64-linux";
      networking.hostName = "aoide-songs";
      boot.loader.grub.enable = false;
      fileSystems."/" = {
        device = "none";
        fsType = "tmpfs";
      };
      system.stateVersion = "25.11";
    };
}
