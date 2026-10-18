# tests/selection/rices-clash/strillix/rice.nix — a rice folder named like an
# aggregation. The hook refuses it when the registry is read, naming the clash.
{
  system.module = _: {
    aoide.theme.palette.bg = "#000000";
  };
}
