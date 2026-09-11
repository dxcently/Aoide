# modules/nucleus/default.nix — every unconditional core file this tree
# ships.
#
# One line per file, in directory order. A new nucleus file is core
# plumbing, not a toggle — it lands here the same way, plus one line.
{
  imports = [
    ./aoided.nix
    ./config.nix
    ./melete-adapter.nix
    ./nix.nix
    ./options.nix
    ./packages.nix
    ./secrets.nix
    ./shellbridge.nix
  ];
}
