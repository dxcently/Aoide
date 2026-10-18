# tests/consumer/rices/solo/_widgets/default.nix — the shelf roll-up, in the
# shape every real rice's is (`rice.mkWidget` binds the owner, so a record owned
# by "solo" can only be minted here).
{ lib, rice, ... }:
let
  owner = "solo";

  slotFiles = lib.filterAttrs (n: t: t == "regular" && lib.hasSuffix ".nix" n && n != "default.nix") (
    builtins.readDir ./.
  );

  slotOf = n: lib.removeSuffix ".nix" n;
in
lib.mapAttrs' (
  n: _: lib.nameValuePair (slotOf n) (rice.mkWidget owner (slotOf n) (import (./. + "/${n}")))
) slotFiles
