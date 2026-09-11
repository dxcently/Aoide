# modules/default.nix — the AoideOS module tree, one explicit aggregate.
#
# Three layers, in the order the module system merges them: dendrites
# (opt-in), facets (render surfaces), nucleus (unconditional core). Each
# subdirectory names its own files and nothing else's — no file is ever
# named from outside the directory that holds it.
{
  imports = [
    ./dendrites
    ./facets
    ./nucleus
  ];
}
