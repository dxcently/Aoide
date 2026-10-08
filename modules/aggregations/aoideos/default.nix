# aoideos — AoideOS itself: the paint, the session it paints on, and the songs.
#
# The whole desktop in one membership. It does NOT include `desktop` (audio,
# clipboard, notifications, screenshots — a graphical session's platform) or
# `agents`: a machine can carry the shell and the songs without the desktop's
# app set, and without a single coding agent. Composing the two is the host's
# line to write, not this body's to guess.
#
# The SONGS are not a member. A host names them in its own module
# (`habit.song.declared` / `habit.song.available`), because whether a `rice.nix`
# is imported is decided in the constructor's selection pass — before any
# aggregation body is read — and only the host module's `habit.*` keys are
# available that early. `lyra` rides here
# because a host performing a song without a performer is refused (with a
# message naming this aggregation as the fix).
#
# The provider choice is the aggregation's, so a host says which compositor it
# runs in one place (`aggregation.aoideos.compositor.provider = "…"`) rather
# than reaching for a separate `dendrites.compositor.provider` line — and the
# same for the WALLPAPER SETTER it paints picks with
# (`aggregation.aoideos.wallpaper.provider = "skwd-wall"`), whose default here
# is the shell's own layer. mkDefault: a host naming a provider still wins.
{
  description = "The AoideOS desktop: compositor, greeter, theme, shell, wallpaper setter, and the committed songs.";

  system = {
    members = [
      "compositor"
      "greeter"
      "hyprland"
      "quickshell"
      "lyra"
      "stylix"
      "wallpaper"
    ];

    providers = {
      compositor = "hyprland";
      wallpaper = "quickshell";
    };
  };
}
