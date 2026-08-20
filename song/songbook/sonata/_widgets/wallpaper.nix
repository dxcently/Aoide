# sonata's `wallpaper` slot — the background layer body, ported from the
# facet's `AoideWallpaper.qml` (its own header: "the facet original stays in
# place until a later phase retires it"). `kind` absent → null: no
# `WidgetSlot { slot: "wallpaper" }` anchor is wired yet — `shell.qml` still
# instantiates the facet's own `AoideWallpaper` directly — so this slot
# registers nothing today. No sibling slots embedded, no uppercase helper
# instantiated.
_: {
  file = "wallpaper.qml";
}
