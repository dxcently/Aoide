# sonata's `calendar` slot — the calendar surface, a window of its own that the
# bar's clock opens through the bridge (`lyra component calendar toggle`).
# `kind` absent → null (a shell-anchored surface, never a declared registry entry).
#
# `helpers`: calendar.qml is the window (SteleLayerPopout) and hosts the stele
# body, CalendarStele.qml, which instantiates `MorphState { ... }` (the shared
# open/close morph state machine).
_: {
  file = "calendar.qml";

  helpers = [
    "CalendarStele.qml"
    "MorphState.qml"
    "SteleLayerPopout.qml"
  ];
}
