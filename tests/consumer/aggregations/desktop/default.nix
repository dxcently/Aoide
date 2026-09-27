# tests/consumer/aggregations/desktop/default.nix — the consumer's own
# aggregation.
#
# An aggregation is the CONSUMER's data: AoideOS's own groupings (`aoideos`,
# `base`, `desktop`, `agents`) describe AoideOS's machines, and a stranger's
# registry carries its own. This one names the paint trio the fixture's contract
# asks for: the shell, the songs it paints, and the theme underneath.
{
  description = "The consumer's desktop: shell, songs, theme.";

  system = {
    members = [
      "lyra"
      "quickshell"
      "stylix"
    ];
  };
}
