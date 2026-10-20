# modules/dendrites/strillix/bar.nix — the `bar` slot's strillix provider: the
# Quickshell bar on the strillix runtime, which it brings by path (README.md).
{
  imports = [ ./runtime.nix ];
  habit.home.imports = [ ./runtime-home.nix ];
  strillix.components.bar.provider = "strillix";
  strillix.surfaces.bar.owner = "quickshell";
}
