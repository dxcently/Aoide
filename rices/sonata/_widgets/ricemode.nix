# sonata's `ricemode` slot — the shared rice-mode control every bar embeds,
# one `WidgetSlot { slot: "ricemode" }` in the bar's right stave. `kind` absent
# → null: it is a bar-embedded WidgetSlot, never a declared `layout.widgets`
# entry. sonata's body is the floor for every rice that does not dress the slot.
# No sibling slots embedded, no uppercase helper instantiated.
_: { file = "ricemode.qml"; }
