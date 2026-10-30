# modules/dendrites/strillix/bar.nix — the `bar` slot's strillix provider: the
# Quickshell bar on the strillix runtime, which it brings by path (README.md).
let
  claim = {
    components.bar.provider = "strillix";
    surfaces.bar.owner = "quickshell";
  };
in
{
  imports = [ ./runtime.nix ];
  habit.home.strillix = claim;
  habit.home.imports = [ ./runtime-home.nix ];
  strillix = claim;
}
