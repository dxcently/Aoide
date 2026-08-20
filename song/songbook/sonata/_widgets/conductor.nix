# sonata's `conductor` slot — the agent-roster temple. Self-contained: its own
# header says "nothing here is shared with TerminalsGadget", and it neither
# instantiates one of the seven uppercase helpers nor embeds another slot.
#
# `kind` absent → null. No anchor embeds `WidgetSlot { slot: "conductor" }`
# in the SHIPPED dock yet (`AoidePanel.qml` still hosts the facet's own
# `ConductorGadget.qml`) — only the ported `widgets/dock.qml` does, and that
# file has no anchor of its own yet either. Until a real anchor is wired this
# slot registers nothing either way, which is exactly what `kind = null`
# states.
_: {
  file = "conductor.qml";
}
