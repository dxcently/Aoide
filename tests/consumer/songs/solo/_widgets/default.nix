# tests/consumer/songs/solo/_widgets/default.nix — the shelf roll-up, in the
# shape every real song's is (`song.mkWidget` binds the owner, so a record owned
# by "solo" can only be minted here).
{ lib, song, ... }:
let
  owner = "solo";

  slotFiles = lib.filterAttrs (
    n: t: t == "regular" && lib.hasSuffix ".nix" n && n != "default.nix"
  ) (builtins.readDir ./.);

  slotOf = n: lib.removeSuffix ".nix" n;
in
lib.mapAttrs' (
  n: _: lib.nameValuePair (slotOf n) (song.mkWidget owner (slotOf n) (import (./. + "/${n}")))
) slotFiles
