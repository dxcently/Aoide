# tests/consumer/hosts/consumer.nix — the fixture's host record.
#
# A consumer's host record answers the same interface AoideOS's do, because it is
# the same constructor: aggregations it selects, songs it performs and builds
# in, people, then its own platform settings. `song.declared` is the song it
# PERFORMS; `song.available` holds the other song it builds in — which borrows
# the performer's widgets, so the borrow closure is exercised on a consumer's own
# songbook rather than on Aoide's.
{
  aggregation.desktop.enable = true;

  song.declared = "solo";
  song.available = [ "duet" ];

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

      # The floors a real machine has; a toplevel wants them.
      nixpkgs.hostPlatform = "x86_64-linux";
      networking.hostName = "consumer";
      boot.loader.grub.enable = false;
      fileSystems."/" = {
        device = "none";
        fsType = "tmpfs";
      };
      system.stateVersion = "25.11";
    };
}
