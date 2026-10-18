# rices/sonata/_widgets/default.nix — the widget-record roll-up.
#
# The ONLY file in this shelf that names the rice. Every sibling `<slot>.nix`
# is a plain function that never types "sonata" or its own slot name — this
# file discovers them with `builtins.readDir` (no hand-maintained list to
# drift against the directory) and binds `owner`/`slot` through `mkWidget`,
# the three-arity builder from `lib/rice.nix`.
#
# Returns a RESOLVED `slot -> record` attrset (every widget function already
# applied) — the shape `lib/rice.nix`'s `composeRice` expects as input.
#
# `_widgets/` is underscore-prefixed on purpose: the rices's typed walk
# (`lib/rices.nix`'s `strayNixFiles`) prunes it by name, so this shelf never
# reaches the module system and
# `checks.rice-shape` never sees it. It is score, not a module.
{ lib, rice, ... }:
let
  owner = "sonata";

  slotFiles = lib.filterAttrs (n: t: t == "regular" && lib.hasSuffix ".nix" n && n != "default.nix") (
    builtins.readDir ./.
  );

  slotOf = n: lib.removeSuffix ".nix" n;
in
lib.mapAttrs' (
  n: _: lib.nameValuePair (slotOf n) (rice.mkWidget owner (slotOf n) (import (./. + "/${n}")))
) slotFiles
