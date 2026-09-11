# modules/facets/default.nix — every render surface this tree ships.
#
# One line per directory, in directory order. A new facet is a new
# directory plus one line here; deleting both removes it without a trace.
{
  imports = [
    ./compositor
    ./quickshell
    ./stylix
  ];
}
