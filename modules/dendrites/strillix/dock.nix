# modules/dendrites/strillix/dock.nix — the `dock` slot's strillix provider: the
# Quickshell dock on the strillix runtime, which it brings by path (README.md).
let
  claim = {
    components.dock.provider = "strillix";
  };
in
{
  imports = [ ./runtime.nix ];
  habit.home.strillix = claim;
  habit.home.imports = [ ./runtime-home.nix ];
  strillix = claim;
}
