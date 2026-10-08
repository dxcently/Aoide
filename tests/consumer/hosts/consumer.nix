# tests/consumer/hosts/consumer.nix — the fixture's host module.
#
# A consumer's host answers the same interface AoideOS's do, because it is
# the same constructor: aggregations it selects, songs it performs and builds
# in, people, then its own platform settings. `habit.song.declared` is the song it
# PERFORMS; `habit.song.available` holds the other song it builds in — which borrows
# the performer's widgets, so the borrow closure is exercised on a consumer's own
# songbook rather than on Aoide's.
_: {
  habit.aggregation.desktop.enable = true;

  habit.song.declared = "solo";
  habit.song.available = [ "duet" ];

  habit.users.fixture = {
    definition = ../user.nix;
    home.enable = true;
  };

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
}
