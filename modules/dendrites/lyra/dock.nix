# modules/dendrites/lyra/dock.nix — the `dock` slot's vibeos provider: the
# Quickshell dock on the vibeos runtime, which it brings by path (README.md).
{
  imports = [ ./runtime.nix ];
  habit.home.imports = [ ./runtime-home.nix ];
  aoide.components.dock.provider = "vibeos";
}
