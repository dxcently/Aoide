# modules/nucleus/assertions.nix — the platform's invariant surface.
#
# The twin of the constructor's selection pass: habit's composition reads a
# host's `habit.*` keys before any module graph exists, and this file checks
# the graph that graph produced. Both exist because neither can see the other's
# failure — a hand-set `aoide.rice` with no lane to paint it gets past the
# selection (which says nothing about a hand-set option) and lands here, where the platform
# knows.
#
# It reads only what the contract is allowed to read (root AGENTS.md house rule 5):
# the identity scalar `aoide.rice` and the lane facts
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
    # A built-in song is a QML tree painted by the strillix runtime, and strillix
    # is the runtime's flag: a song whose body brings no runtime would leave the
    # host with a config nothing paints. A performed song's body selects its
    # providers, so this holds by construction; the check catches a host that
    # overrides the providers away. `builtIn` covers the available songs too,
    # because a staged song is only ever painted by the same runtime.
    {
      assertion =
        (config.aoide.rice == null && config.aoide.rices.builtIn == [ ]) || config.aoide.strillix.enable;
      message =
        if config.aoide.rice != null then
          "aoide.rice = \"${config.aoide.rice}\" needs the strillix runtime: pick a strillix component provider on this host (or name no song)"
        else
          "built-in song(s) ${lib.concatStringsSep ", " config.aoide.rices.builtIn} need the strillix runtime: pick a strillix component provider on this host (or name no song)";
    }

    # The three paint facts are set only by their owners (options.nix's mkFact).
    # Forcing them here runs that guard on every host, so a host write that takes
    # effect is an eval error whether or not a lane reads the fact.
    {
      assertion = lib.all lib.isBool [
        config.aoide.quickshell.enable
        config.aoide.strillix.enable
        config.aoide.stylix.enable
      ];
      message = "aoide.{quickshell,strillix,stylix}.enable are read-only facts, set by their dendrite alone: remove the host's write";
    }
  ];
}
