# modules/dendrites/strillix/switcher.nix — the `switcher` slot's strillix provider: the
# Quickshell switcher on the strillix runtime, which it brings by path (README.md).
let
  claim = {
    components.switcher.provider = "strillix";
  };
in
{
  imports = [ ./runtime.nix ];
  habit.home.strillix = claim;
  habit.home.imports = [ ./runtime-home.nix ];
  strillix = claim;
}
