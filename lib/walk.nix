# lib/walk.nix — the dendritic walker.
#
# Hands every `.nix` file under `dir` to the module system, so a file placed
# under a walked directory self-registers with no import list to maintain.
# A path containing `/_` is the shelving opt-out: prefix a filename (or a
# directory) with `_` to hide it from discovery without deleting it. This is
# the same mechanism dxflake uses (see entities/dxflake in the wiki).
#
# Usage:  walk = import ./lib/walk.nix { inherit lib; };
#         modules = walk ./modules;
{ lib }:
dir:
builtins.filter (
  p:
  let
    s = toString p;
  in
  lib.hasSuffix ".nix" s && !(lib.hasInfix "/_" s)
) (lib.filesystem.listFilesRecursive dir)
