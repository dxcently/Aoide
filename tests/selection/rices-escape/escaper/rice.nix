# tests/selection/rices-escape/escaper/rice.nix — a rice that reaches out of
# its own folder: its text names `../../../lib/rice.nix`, the path it would
# import instead of taking `rice` as an argument.
#
# Paired with `riceEscapeClean`: the fixture rices the other rice cases use
# has no escaping `.nix`, and this one's `../` is what `checks.rice-shape` must
# name. Nothing imports the file — the case reads `escapingNixFiles`, which is a
# text scan — so it never has to resolve, and it deliberately does not.
{
  system.module = _: {
    aoide.theme.palette.bg = "#000000";
  };
}
