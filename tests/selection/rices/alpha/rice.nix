# tests/selection/rices/alpha/rice.nix — a plain rice body, for the rices cases.
#
# The shape every committed rice.nix has (CONTRACTS.md §5): one attrset with a
# `system` and a `home` half, read only when the rice is selected. It configures
# nothing — these cases are about DISCOVERY and SELECTION, not about what a rice paints.
{
  system.module = _: {
    strillix.theme.palette.bg = "#000000";
  };
}
