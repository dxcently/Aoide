# song/songbook/sonata/_widgets/default.nix — the widget-record roll-up.
#
# The ONLY file in this shelf that names the song. Every sibling `<slot>.nix`
# is a plain function that never types "sonata" or its own slot name — this
# file discovers them with `builtins.readDir` (no hand-maintained list to
# drift against the directory) and binds `owner`/`slot` through `mkWidget`,
# the three-arity builder from `lib/song.nix`.
#
# Returns a RESOLVED `slot -> record` attrset (every widget function already
# applied) — the shape `lib/song.nix`'s `composeSong` expects as input.
#
# `_widgets/` is underscore-prefixed on purpose: `lib/walk.nix` drops any path
# containing `/_`, so this shelf never reaches the module system and
# `checks.song-shape` never sees it. It is score, not a module.
{ lib }:
let
  song = import ../../../../lib/song.nix { inherit lib; };
  owner = "sonata";

  slotFiles = lib.filterAttrs (n: t: t == "regular" && lib.hasSuffix ".nix" n && n != "default.nix") (
    builtins.readDir ./.
  );

  slotOf = n: lib.removeSuffix ".nix" n;
in
lib.mapAttrs' (
  n: _: lib.nameValuePair (slotOf n) (song.mkWidget owner (slotOf n) (import (./. + "/${n}")))
) slotFiles
