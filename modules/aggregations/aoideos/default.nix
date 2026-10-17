# aoideos — AoideOS's desktop floor: the compositor, the theme, and the session
# it paints on. The paint and the songs are NOT members.
#
# The paint is the SONG's. Each committed `rice.nix` is an aggregation body
# (`lib/songbook.nix`'s hook): its `system` half names the ten vibeos component
# providers the Quickshell desktop is made of (bar to greeter) and its `home`
# half names the `lyra` renderer that writes its Hyprland rice. A host performs
# one song (`habit.song.declared`), and that song's body brings the paint, so a
# host that wants a desktop names a song, not a list of components here.
#
# What this group keeps is what is true of any desktop whatever it performs: the
# compositor, its behaviour and the theme underneath. A host says which
# compositor it runs (`aggregation.aoideos.compositor.provider = "…"`), and a
# host that wants the external wallpaper engine names it on the wallpaper
# dendrite (`habit.dendrites.wallpaper.provider = "skwd-wall"`), which a song's
# body would otherwise default to its own `vibeos` setter.
{
  description = "The AoideOS desktop floor: the compositor, its behaviour, and the theme. The paint comes from the performed song.";

  system = {
    members = [
      "hyprland"
      "stylix"
    ];

    providers.compositor = "hyprland";
  };
}
