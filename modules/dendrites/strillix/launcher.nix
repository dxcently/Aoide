# modules/dendrites/strillix/launcher.nix — the `launcher` slot's strillix provider: the
# Quickshell launcher on the strillix runtime, which it brings by path (README.md).
let
  claim = {
    components.launcher.provider = "strillix";
    surfaces.launcher.owner = "quickshell";
  };
in
{
  imports = [ ./runtime.nix ];
  habit.home.strillix = claim;
  habit.home.imports = [ ./runtime-home.nix ];
  strillix = claim;
}
