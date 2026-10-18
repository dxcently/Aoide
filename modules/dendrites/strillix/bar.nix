# modules/dendrites/strillix/bar.nix — the `bar` slot's strillix provider: the
# Quickshell bar on the strillix runtime, which it brings by path (README.md).
{
  imports = [ ./runtime.nix ];
  habit.home.imports = [ ./runtime-home.nix ];
  aoide.components.bar.provider = "strillix";
  aoide.surfaces.bar.owner = "quickshell";
}
