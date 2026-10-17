# tests/consumer/stub-lock.nix — the consumer's OWN lock provider, never selected.
#
# The consumer's `lock` slot names this stub beside Aoide's vibeos part, and no
# host in this fixture picks the stub: habit must never import this file. It
# throws on read, so a host that did read it would fail its own evaluation. That
# is the proof an unselected provider is never touched.
throw "stub-lock.nix was read, but no consumer host selects the lock stub"
