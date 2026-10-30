# modules/dendrites/strillix/powermenu.nix — the `powermenu` slot's strillix provider: the
# Quickshell powermenu on the strillix runtime, which it brings by path (README.md).
let
  claim = {
    components.powermenu.provider = "strillix";
  };
in
{
  imports = [ ./runtime.nix ];
  habit.home.strillix = claim;
  habit.home.imports = [ ./runtime-home.nix ];
  strillix = claim;
}
