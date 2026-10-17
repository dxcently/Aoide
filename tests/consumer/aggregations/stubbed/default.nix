# tests/consumer/aggregations/stubbed/default.nix — a consumer's own mix of
# value slots: its `bar` provider beside Aoide's `dock` one.
#
# The slot names are the consumer's own catalogue entries (flake.nix's `slots`),
# so this group picks a provider by name from a set the consumer wrote, and
# the vibeos one is Aoide's part. Nothing here reads a path into Aoide.
{
  description = "The consumer's bar (its own provider) beside Aoide's dock.";

  system = {
    members = [
      "bar"
      "dock"
    ];

    providers = {
      bar = "stub";
      dock = "vibeos";
    };
  };
}
