# modules/dendrites/strillix/powermenu.nix — the `powermenu` slot's strillix provider: the
# Quickshell powermenu on the strillix runtime, which it brings by path (README.md).
{
  imports = [ ./runtime.nix ];
  habit.home.imports = [ ./runtime-home.nix ];
  aoide.components.powermenu.provider = "strillix";
}
