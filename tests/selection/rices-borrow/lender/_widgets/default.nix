# tests/selection/rices-borrow/lender/_widgets/default.nix — the shelf
# roll-up, in the shape the real shelves use (`rice.mkWidget` binds the owner, so
# a record owned by "lender" can only be minted here).
{ lib, rice, ... }:
let
  owner = "lender";

  slotFiles = lib.filterAttrs (n: t: t == "regular" && lib.hasSuffix ".nix" n && n != "default.nix") (
    builtins.readDir ./.
  );

  slotOf = n: lib.removeSuffix ".nix" n;
in
lib.mapAttrs' (
  n: _: lib.nameValuePair (slotOf n) (rice.mkWidget owner (slotOf n) (import (./. + "/${n}")))
) slotFiles
