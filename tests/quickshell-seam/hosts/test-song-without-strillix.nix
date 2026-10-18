# tests/quickshell-seam/hosts/test-song-without-strillix.nix — a song with no
# performer.
#
# `aoide.song` is a FACT: it says "perform this rice". A host that says so and
# takes no strillix runtime has nothing to build the QML from, deploy it, seed the
# stage or restart — the failure is silent everywhere else (the body that would
# have brought the runtime is never imported for a hand-set fact, and no other
# lane reads the song), so the platform asserts on it. This fixture is the eval
# that must FAIL: one host, one hand-set fact, and the platform assertion's own
# bytes as the evidence.
_: {
  # The services the nucleus aggregation carries, by name: the daemon's units
  # and the `aoide` binaries are what the absences below are read against.
  habit.dendrites.aoided.enable = true;
  habit.dendrites.aoide-packages.enable = true;

  habit.dendrites.quickshell.enable = true;

  habit.users.khoa = {
    definition = ../user.nix;
    home.enable = true;
  };

  aoide.enable = true;
  aoide.song = "sonata";
  aoide.quickshell.config = toString ../fixture;

  networking.hostName = "test-song-without-strillix";
  boot.loader.grub.enable = false;
  fileSystems."/" = {
    device = "none";
    fsType = "tmpfs";
  };
  system.stateVersion = "25.11";
}
