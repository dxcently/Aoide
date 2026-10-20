# tests/selection/rices-borrow/lender/rice.nix — a rice body for the borrow cases.
#
# Minimal on purpose: the borrow cases are about OWNERSHIP, not about what a rice paints.
{
  system.module = _: {
    strillix.theme.palette.bg = "#000000";
  };
}
