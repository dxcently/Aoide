# modules/dendrites/lyra/calendar.nix — the `calendar` slot's vibeos provider: the
# Quickshell calendar on the vibeos runtime, which it brings by path (README.md).
{
  imports = [ ./runtime.nix ];
  habit.home.imports = [ ./runtime-home.nix ];
  aoide.components.calendar.provider = "vibeos";
}
