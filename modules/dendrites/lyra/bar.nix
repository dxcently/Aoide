# modules/dendrites/lyra/bar.nix — the `bar` slot's vibeos provider: the
# Quickshell bar on the vibeos runtime, which it brings by path (README.md).
{
  imports = [ ./runtime.nix ];
  habit.home.imports = [ ./runtime-home.nix ];
  aoide.components.bar.provider = "vibeos";
  aoide.surfaces.bar.owner = "quickshell";
}
