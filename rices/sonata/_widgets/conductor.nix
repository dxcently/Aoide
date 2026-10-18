# sonata's `conductor` slot — the agent-roster temple. Self-contained: its own
# header says "nothing here is shared with TerminalsGadget", and it neither
# instantiates one of the seven uppercase helpers nor embeds another slot.
#
# `kind` absent → null: shell-anchored, never a declared registry entry.
# Hosted by sonata's own `widgets/dock.qml` (the shipped dock — the shell's
# former `AoidePanel.qml` is retired), one `WidgetSlot { slot: "conductor" }`
# in its gadget column.
_: {
  file = "conductor.qml";
}
