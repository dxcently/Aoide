# sonata's `powermenu` slot — hosted by shell.qml's `SurfaceSlot { slot:
# "powermenu" }` (slots.md's wired table). `kind` absent → null: facet-
# anchored, never a declared registry entry.
#
# No `packages`: the actual reboot/shutdown action goes through
# `root.bridge.sendCommand({ cmd: "power", action })` — the QML never shells
# out directly (its own header: "QML never [does it directly]").
_: {
  file = "powermenu.qml";
}
