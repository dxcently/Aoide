# tests/standalone-home/hosts/rice.nix — the standalone host's habit selections:
# the aggregations, the performed rice and the slot providers. habit reads these
# from the host file itself (its scan does not follow imports), so they live here.
{ lib, ... }:
{
  habit.aggregation.desktop.enable = true;
  habit.aggregation.strillix.enable = true;
  habit.rice.declared = "barcarolle";

  habit.dendrites =
    lib.genAttrs
      [
        "bar"
        "calendar"
        "dock"
        "launcher"
        "lock"
        "notifications"
        "powermenu"
        "switcher"
      ]
      (_: {
        enable = true;
        provider = "strillix";
      })
    // {
      wallpaper = {
        enable = true;
        provider = "strillix";
      };
    };
}
