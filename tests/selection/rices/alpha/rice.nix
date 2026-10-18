# tests/selection/rices/alpha/rice.nix — a plain song body, for the rices cases.
#
# The shape every committed rice.nix has (CONTRACTS.md §5): one attrset with a
# `system` and a `home` half, read only when the song is selected. It configures
# nothing — these cases are about DISCOVERY and SELECTION, not about what a song paints.
{
  system.module = _: {
    aoide.theme.palette.bg = "#000000";
  };
}
