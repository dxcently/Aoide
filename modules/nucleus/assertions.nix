# modules/nucleus/assertions.nix — the platform's invariant surface.
#
# The twin of the constructor's selection pass: habit's composition reads a
# host's `habit.*` keys before any module graph exists, and this file checks
# the graph that graph produced. Both exist because neither can see the other's
# failure — a hand-set `aoide.song` with no lane to paint it gets past the
# selection (which says nothing about a hand-set option) and lands here, where the platform
# knows.
#
# It reads only what the contract is allowed to read (root AGENTS.md house rule 5):
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
    # derived FACT (a host's `habit.song.declared` produces it), so the thing
    # to change is which lane the host takes.
    {
      assertion = config.aoide.song == null || config.aoide.lyra.enable;
      message = "aoide.song = \"${config.aoide.song}\" needs the lyra dendrite: select habit.aggregation.aoideos (or habit.dendrites.lyra) on this host";
    }

    # The three paint facts are set only by their owners (options.nix's mkFact).
    # Forcing them here runs that guard on every host, so a host write that takes
    # effect is an eval error whether or not a lane reads the fact.
    {
      assertion = lib.all lib.isBool [
        config.aoide.quickshell.enable
        config.aoide.lyra.enable
        config.aoide.stylix.enable
      ];
      message = "aoide.{quickshell,lyra,stylix}.enable are read-only facts, set by their dendrite alone: remove the host's write";
    }
  ];
}
