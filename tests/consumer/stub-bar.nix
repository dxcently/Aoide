# tests/consumer/stub-bar.nix — the consumer's OWN bar provider, selected.
#
# A provider is a plain module: this one names itself in the component record
# and brings no runtime, so the host that selects it has a bar slot filled and
# no Quickshell at all (hosts/stubbed.nix pairs it with Aoide's dock, which
# brings the runtime). `aoide.components` is the contract's record of what a
# host filled, so the fixture reads the stub's name back from it.
{
  aoide.components.bar.provider = "stub";
}
