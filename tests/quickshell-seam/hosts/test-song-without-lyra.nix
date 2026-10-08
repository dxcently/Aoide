# tests/quickshell-seam/hosts/test-song-without-lyra.nix — a song with no
# performer.
#
# `aoide.song` is a FACT derived from the host's `habit.song.declared`: it says
# "perform this rice". A host that says so and takes no `lyra` lane has nothing
# to build the QML from, deploy it, seed the stage or restart — the failure is
# silent everywhere else (every `rice.nix` self-gates on its own name, and no
# other lane reads the song), so the platform asserts on it. This fixture is the
# eval that must FAIL: one host, one selection, and the platform assertion's own
# bytes as the evidence.
_: {
  habit.dendrites.quickshell.enable = true;

  habit.users.khoa = {
    definition = ../user.nix;
    home.enable = true;
  };

  aoide.enable = true;
  aoide.song = "sonata";
  aoide.quickshell.config = toString ../fixture;

  networking.hostName = "test-song-without-lyra";
  boot.loader.grub.enable = false;
  fileSystems."/" = {
    device = "none";
    fsType = "tmpfs";
  };
  system.stateVersion = "25.11";
}
