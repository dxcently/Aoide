# modules/dendrites/lyra/launcher.nix — the `launcher` slot's vibeos provider: the
# Quickshell launcher on the vibeos runtime, which it brings by path (README.md).
{
  imports = [ ./runtime.nix ];
  habit.home.imports = [ ./runtime-home.nix ];
  aoide.components.launcher.provider = "vibeos";
  aoide.surfaces.launcher.owner = "quickshell";
}
