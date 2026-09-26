# modules/overrides/default.nix — the override discovery point.
#
# Every `*.nix` file beside this one is a record; this file produces
# `name = path` and imports nothing. A record IS read by the constructor on
# every host, because matching means reading which dendrites it targets. What an
# unmatched record never costs is its work: `overlay` and the lane modules are
# functions, and nothing calls them.
#
# Empty is a real answer: the directory can hold nothing but this file, and
# `overridesFor` returns no match, no overlay and no module for every host.
let
  entries = builtins.readDir ./.;

  isRecord =
    name:
    entries.${name} == "regular" && name != "default.nix" && builtins.match ".*\\.nix" name != null;

  names = builtins.filter isRecord (builtins.attrNames entries);
in
builtins.listToAttrs (
  map (name: {
    name = builtins.substring 0 (builtins.stringLength name - 4) name;
    value = ./. + "/${name}";
  }) names
)
