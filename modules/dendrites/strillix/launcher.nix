# modules/dendrites/strillix/launcher.nix — the `launcher` slot's strillix provider: the
# Quickshell launcher on the strillix runtime, which it brings by path (README.md).
{
  imports = [ ./runtime.nix ];
  habit.home.imports = [ ./runtime-home.nix ];
  aoide.components.launcher.provider = "strillix";
  aoide.surfaces.launcher.owner = "quickshell";
}
