# modules/dendrites/default.nix — the whole-tree aggregate.
#
# Its imports are DERIVED from `modules/default.nix`'s catalogue: that record is
# the one place a dendrite file is named (root `AGENTS.md` house rule 7), so
# this file names no dendrite at all. Adding one is one catalogue line and
# nothing here changes; deleting the line shelves it, file untouched.
#
# A dendrite file is a lane record (`{ body; nixos; }`, CONTRACTS.md §2).
# Taking this aggregate merges every `body`, so a host that takes the whole tree
# still sees each `aoide.<name>.*` option and its own guard; a host the
# constructor (`lib/composition.nix`) assembles imports only the `nixos` lane of
# what it selected. The two are mutually exclusive in one module list — an
# aggregate `body` and a selected lane's own `body` are the same declarations
# twice, which nixpkgs throws on rather than merging.
#
# This aggregate is on its way out (S7): once hosts select through the
# constructor, nothing imports a whole tree, and this file goes with them.
{
  imports = map (p: (import p).body) (builtins.attrValues (import ../default.nix).catalogue);
}
