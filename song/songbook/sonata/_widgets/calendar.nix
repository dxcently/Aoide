# sonata's `calendar` slot — the clock's popout body, hosted inside bar.qml's
# own `WidgetSlot { slot: "calendar" }`. `kind` absent → null (facet-anchored,
# via `bar`, not shell.qml directly — but never a declared registry entry).
#
# `helpers`: calendar.qml instantiates `MorphState { ... }` (the shared
# open/close morph state machine). AudioColonnade/GadgetFrame/SteleLayerPopout/
# StelePopout are named only in header comments recounting design history
# ("the 2026-07-31 standing direction"); none is actually instantiated.
_: {
  file = "calendar.qml";

  helpers = [ "MorphState.qml" ];
}
