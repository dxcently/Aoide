# tests/consumer/aggregations/default.nix — the consumer's aggregation
# discovery, the same one-level scan AoideOS's registry uses: every child
# directory holding a `default.nix` is an aggregation, named by its directory,
# and NO body is imported here (the constructor imports only the bodies a host
# selected).
let
  entries = builtins.readDir ./.;
  names = builtins.filter (
    name: entries.${name} == "directory" && builtins.pathExists (./. + "/${name}/default.nix")
  ) (builtins.attrNames entries);
in
builtins.listToAttrs (
  map (name: {
    inherit name;
    value = ./. + "/${name}";
  }) names
)
