# modules/dendrites/strillix/notifications.nix — the `notifications` slot's strillix
# provider: the Quickshell notification surface on the strillix runtime, which it
# brings by path (README.md).
#
# dunst owns notification DELIVERY — the bus name, history, stacking and the
# pause levels — but it draws NOTHING (`skip_display` on every rule;
# modules/dendrites/dunst.nix). Quickshell owns the SURFACE: the `herald` popup
# slot and the dock's `herald-center` ledger, both drawn from stage/herald.json.
# Stylix's stand-down keys on "not stylix", so its dunst/mako targets stay
# disabled either way — which is still what we want, since a Stylix-generated
# dunstrc would fight the dendrite's.
{
  imports = [ ./runtime.nix ];
  habit.home.imports = [ ./runtime-home.nix ];
  aoide.components.notifications.provider = "strillix";
  aoide.surfaces.notifications.owner = "quickshell";
}
