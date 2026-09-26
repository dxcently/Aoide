# aoideos — AoideOS itself: the paint, the session it paints on, and the songs.
#
# The whole desktop in one membership. It does NOT include `desktop` (audio,
# clipboard, notifications, screenshots — a graphical session's platform) or
# `agents`: a machine can carry the shell and the songs without the desktop's
# app set, and without a single coding agent. Composing the two is the host's
# line to write, not this body's to guess.
#
# `songbook` rides here because songs are the desktop's business and nothing
# else's; a host that selects no `aoideos` gets no committed song imported at
# all, and one that keeps `aoideos` but wants no song drops the member.
#
# The provider choice is the aggregation's, so a host says which compositor it
# runs in one place (`aggregation.aoideos.compositor.provider = "…"`) rather
# than reaching for a separate `dendrites.compositor.provider` line. mkDefault:
# a host naming a provider still wins.
{
  description = "The AoideOS desktop: compositor, greeter, theme, shell, and the committed songs.";

  system = {
    members = [
      "compositor"
      "greeter"
      "hyprland"
      "quickshell"
      "lyra"
      "songbook"
      "stylix"
    ];

    providers.compositor = "hyprland";
  };
}
