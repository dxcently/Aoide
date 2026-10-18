# strillix — Strillix's desktop floor: the compositor, the theme, and the session
# it paints on. The paint and the rices are NOT members.
#
# The paint is the RICE's. Each committed `rice.nix` is an aggregation body
# (`lib/rices.nix`'s hook): its `system` half names the ten strillix component
# providers the Quickshell desktop is made of (bar to greeter) and its `home`
# half names the `strillix` renderer that writes its Hyprland rice. A host performs
# one rice (`habit.rice.declared`), and that rice's body brings the paint, so a
# host that wants a desktop names a rice, not a list of components here.
#
# What this group keeps is what is true of any desktop whatever it performs: the
# compositor, its behaviour and the theme underneath. A host says which
# compositor it runs (`aggregation.strillix.compositor.provider = "…"`), and a
# host that wants the external wallpaper engine names it on the wallpaper
# dendrite (`habit.dendrites.wallpaper.provider = "skwd-wall"`), which a rice's
# body would otherwise default to its own `strillix` setter.
{
  description = "The Strillix desktop floor: the compositor, its behaviour, and the theme. The paint comes from the performed rice.";

  system = {
    members = [
      "hyprland"
      "stylix"
    ];

    providers.compositor = "hyprland";
  };
}
