# tests/selection/rices/_shelved/rice.nix — shelved, like a `_`-prefixed
# dendrite: discovered by nothing, selectable by nothing.
{
  system.module = _: {
    aoide.theme.palette.bg = "#000000";
  };
}
