# modules/nucleus/assertions.nix — the platform's invariant surface.
#
# The twin of the constructor's gate pass: habit's composition checks a host
# RECORD before any module graph exists, and this file checks the graph that
# graph produced. Both exist because neither can see the other's failure — a
# hand-set `aoide.song` with no lane to paint it gets past the record (the
# record says nothing about songs today) and lands here, where the platform
# knows.
#
# It reads only what nucleus is allowed to read (root AGENTS.md house rule 5):
# the identity scalar `aoide.song` and the lane facts
# `modules/nucleus/options.nix` declares. It declares nothing itself, so it
# adds no option, no unit and no package — a host that trips nothing evaluates
# exactly as it did before this file existed.
{
  config,
  lib,
  ...
}:
{
  assertions = [
    # A song is a QML tree painted by lyra, so naming one without the lyra lane
    # is a host that says "perform this rice" and brings no performer: lyra is
    # what builds the config from the song's widgets, deploys it, seeds the
    # stage and installs the rice binary. The failure would otherwise be silent
    # — every committed `rice.nix` self-gates on its own name, the other lanes
    # stay inert, and the host boots with no surface at all.
    #
    # The fix is a selection, not a flag on this option: `aoide.song` is the
    # derived FACT (a host record's `song.declared` produces it), so the thing
    # to change is which lane the host takes.
    {
      assertion = config.aoide.song == null || config.aoide.lyra.enable;
      message = "aoide.song = \"${config.aoide.song}\" needs the lyra dendrite: select aggregations.aoideos (or dendrites.lyra) on this host";
    }
  ];
}
