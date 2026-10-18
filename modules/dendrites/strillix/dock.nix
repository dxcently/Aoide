# modules/dendrites/strillix/dock.nix — the `dock` slot's strillix provider: the
# Quickshell dock on the strillix runtime, which it brings by path (README.md).
{
  imports = [ ./runtime.nix ];
  habit.home.imports = [ ./runtime-home.nix ];
  aoide.components.dock.provider = "strillix";
}
