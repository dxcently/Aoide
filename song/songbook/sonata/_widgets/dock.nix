# sonata's `dock` slot — the center-left codex. `kind` absent → null:
# facet-anchored (`shell.qml`'s `SurfaceSlot { slot: "dock" }`), never a
# declared registry entry — same shape as `powermenu.nix`/`launcher.nix`.
# The facet's own AoidePanel.qml, this slot's predecessor, is retired.
#
# `dependsOn`: dock.qml embeds one `WidgetSlot` per stele in its gadget
# column, in stack order — conductor, terminals, usage (the ACTIVE one, under
# Terminals; the commented-out one under Conductor was removed 2026-08-12 and
# is dead code), meters, power, herald-center.
_: {
  file = "dock.qml";

  dependsOn = [
    "conductor"
    "terminals"
    "usage"
    "meters"
    "power"
    "herald-center"
  ];
}
