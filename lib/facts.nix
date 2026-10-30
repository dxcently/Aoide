# lib/facts.nix — the enable-fact type. `mkFact name` is the option type of a fact
# whose owner is the dendrite the catalogue names for `name` (modules/default.nix).
# Shared by the NixOS option contract (modules/nucleus/options.nix) and by the
# Home Manager declaration of the same contract (modules/nucleus/options.nix is
# imported into both), so a fact is refused the same way in either evaluation.
{ lib }:
let
  inherit (lib) types;
  inherit (import ../modules/default.nix) catalogue;
in
{
  mkFact =
    name:
    let
      owner = toString catalogue.${name};
    in
    types.bool
    // {
      # Read by lib/strillix.nix: a fact is never forwarded to Home Manager.
      isFact = true;
      merge =
        loc: defs:
        let
          # The owner is the file the catalogue names for `name`, or any file
          # under it when the entry is a directory (`strillix = ./dendrites/strillix;`):
          # a lane may import the owner's files by path, and the fact it sets
          # is still written from the owner's directory.
          fromOwnerOrDecl =
            d:
            toString d.file == owner
            || lib.hasPrefix (owner + "/") (toString d.file)
            || lib.hasSuffix "/modules/nucleus/options.nix" (toString d.file);
        in
        if lib.length defs == 1 && fromOwnerOrDecl (lib.head defs) then
          (lib.head defs).value
        else
          throw "The option `${lib.showOption loc}' is a read-only fact, set only by the dendrite that owns it. Read it, never set it.";
    };
}
