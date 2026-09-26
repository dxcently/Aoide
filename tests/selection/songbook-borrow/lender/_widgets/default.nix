# tests/selection/songbook-borrow/lender/_widgets/default.nix — the shelf
# roll-up, in the shape the real shelves use (`song.mkWidget` binds the owner, so
# a record owned by "lender" can only be minted here).
{ lib, song, ... }:
let
  owner = "lender";

  slotFiles = lib.filterAttrs (n: t: t == "regular" && lib.hasSuffix ".nix" n && n != "default.nix") (
    builtins.readDir ./.
  );

  slotOf = n: lib.removeSuffix ".nix" n;
in
lib.mapAttrs' (
  n: _: lib.nameValuePair (slotOf n) (song.mkWidget owner (slotOf n) (import (./. + "/${n}")))
) slotFiles
