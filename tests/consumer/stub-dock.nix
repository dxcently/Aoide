# tests/consumer/stub-dock.nix — the consumer's OWN dock provider, never selected.
#
# The consumer's `dock` slot names this stub beside Aoide's strillix part, and no
# host in this fixture picks the stub: habit must never import this file. It
# throws on read, so a host that did read it would fail its own evaluation. That
# is the proof an unselected provider is never touched.
throw "stub-dock.nix was read, but no consumer host selects the dock stub"
