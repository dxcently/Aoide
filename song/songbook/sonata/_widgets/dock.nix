# sonata's `dock` slot — the center-left codex, ported from the facet's
# AoidePanel.qml (its own header: "the facet original stays in place until a
# later phase retires it"). `kind` absent → null: no `SurfaceSlot { slot:
# "dock" }` anchor is wired yet (that lands with the phase this file's header
# names), so it registers nothing today.
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
