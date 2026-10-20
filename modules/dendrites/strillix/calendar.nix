# modules/dendrites/strillix/calendar.nix — the `calendar` slot's strillix provider: the
# Quickshell calendar on the strillix runtime, which it brings by path (README.md).
{
  imports = [ ./runtime.nix ];
  habit.home.imports = [ ./runtime-home.nix ];
  strillix.components.calendar.provider = "strillix";
}
