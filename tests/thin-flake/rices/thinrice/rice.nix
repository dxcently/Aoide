# tests/thin-flake/rices/thinrice/rice.nix — the thin flake's own book, one rice: the smallest
# body lib/checks.nix's rice-shape check accepts. No host selects it. Its book is merged with
# Strillix's, so the thin host's rices are the merged directory its home lane reads.
{
  description = "The thin flake's rice: a book of its own, selected by no host.";
  home = { };
}
