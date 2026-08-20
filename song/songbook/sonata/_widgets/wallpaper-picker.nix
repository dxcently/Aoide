# sonata's `wallpaper-picker` slot — SUPER+W wallpaper switcher, ported from
# the facet's `AoideWallpaperPicker.qml` (its own header: "the facet original
# stays in place until a later phase retires it"). `kind` absent → null: no
# `SurfaceSlot { slot: "wallpaper-picker" }` anchor is wired yet — `shell.qml`
# still instantiates the facet's own `AoideWallpaperPicker` directly — so it
# registers nothing today.
#
# No `packages`: it runs `Process { command: ["ls", "-1", coversDir] }`
# (coreutils — not a thing a venue can fail to stock, so not worth declaring)
# and `Quickshell.execDetached(["aoide", "cover", "set", path])` (the
# project's own CLI, always present, not a swappable external tool).
#
# `helpers`: instantiates `GadgetFrame { ... }`.
_: {
  file = "wallpaper-picker.qml";

  helpers = [ "GadgetFrame.qml" ];
}
