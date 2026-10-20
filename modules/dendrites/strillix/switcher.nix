# modules/dendrites/strillix/switcher.nix — the `switcher` slot's strillix provider: the
# Quickshell switcher on the strillix runtime, which it brings by path (README.md).
{
  imports = [ ./runtime.nix ];
  habit.home.imports = [ ./runtime-home.nix ];
  strillix.components.switcher.provider = "strillix";
}
