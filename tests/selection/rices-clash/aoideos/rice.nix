# tests/selection/rices-clash/aoideos/rice.nix — a song folder named like an
# aggregation. The hook refuses it when the registry is read, naming the clash.
{
  system.module = _: {
    aoide.theme.palette.bg = "#000000";
  };
}
