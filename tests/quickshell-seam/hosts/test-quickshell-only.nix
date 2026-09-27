# tests/quickshell-seam/hosts/test-quickshell-only.nix — the first lyra-less
# shape: a host that brings its OWN quickshell config.
#
# It selects the `quickshell` dendrite and nothing else from the paint set, and
# it names no song — so the shell it runs is the fixture directory's, started by
# the quickshell lane, with no lyra anywhere in the evaluation (no rice binary,
# no `songs/`, no shellbridge unit, no healthcheck).
{
  dendrites.quickshell.enable = true;

  users.khoa = {
    definition = ../user.nix;
    homeManager.enable = true;
  };

  nixos =
    { ... }:
    {
      aoide.enable = true;
      aoide.quickshell.config = toString ../fixture;

      # The floors a real machine has; a toplevel wants them.
      networking.hostName = "test-quickshell-only";
      boot.loader.grub.enable = false;
      fileSystems."/" = {
        device = "none";
        fsType = "tmpfs";
      };
      system.stateVersion = "25.11";
    };
}
