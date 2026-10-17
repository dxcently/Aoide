# modules/dendrites/lyra/powermenu.nix — the `powermenu` slot's vibeos provider: the
# Quickshell powermenu on the vibeos runtime, which it brings by path (README.md).
{
  imports = [ ./runtime.nix ];
  habit.home.imports = [ ./runtime-home.nix ];
  aoide.components.powermenu.provider = "vibeos";
}
