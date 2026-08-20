# fugue's `herald` slot — the notification popup, hosted by shell.qml's
# `SurfaceSlot { slot: "herald" }`. `kind` absent → null: facet-anchored,
# never a declared `arrangement.widgets` entry.
#
# `dependsOn`: absent. herald.qml declares `required property var livery` and
# `bridge` only — no `WidgetSlot`, no `SurfaceSlot`, no sibling slot addressed
# anywhere but header prose.
#
# `helpers`: herald.qml instantiates `Cell` (fugue's own uppercase helper).
_: {
  file = "herald.qml";

  helpers = [ "Cell.qml" ];
}
