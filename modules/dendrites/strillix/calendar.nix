# modules/dendrites/strillix/calendar.nix — the `calendar` slot's strillix provider: the
# Quickshell calendar on the strillix runtime, which it brings by path (README.md).
let
  claim = {
    components.calendar.provider = "strillix";
  };
in
{
  imports = [ ./runtime.nix ];
  habit.home.strillix = claim;
  habit.home.imports = [ ./runtime-home.nix ];
  strillix = claim;
}
