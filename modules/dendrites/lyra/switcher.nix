# modules/dendrites/lyra/switcher.nix — the `switcher` slot's vibeos provider: the
# Quickshell switcher on the vibeos runtime, which it brings by path (README.md).
{
  imports = [ ./runtime.nix ];
  habit.home.imports = [ ./runtime-home.nix ];
  aoide.components.switcher.provider = "vibeos";
}
