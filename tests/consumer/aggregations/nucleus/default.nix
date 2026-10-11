# tests/consumer/aggregations/nucleus/default.nix — the consumer's own
# `nucleus`.
#
# `nixosModules.nucleus` hands a consumer the option contract and nothing that
# works: the daemon's units, the secrets broker, the nix settings and the
# binaries on the PATH are catalogue entries a consumer selects. This group names
# the four an AoideOS host carries, which is the shape any stranger's flake takes.
# `aoide-config` and `aoide-secrets` stay out: selecting one turns it on, so a
# host that wants the nix-owned config or the broker selects it by name.
{
  description = "The consumer's floor: the aoide services.";

  system = {
    members = [
      "aoide-nix"
      "aoide-packages"
      "aoided"
      "melete-adapter"
    ];
  };
}
