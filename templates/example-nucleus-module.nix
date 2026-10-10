# example-nucleus-module.nix — an option contract every module builds against.
#
# Copy to:  modules/nucleus/<topic>.nix
# Then:     add `./<topic>.nix` to modules/nucleus/default.nix's imports.
# Replace:  <topic> and the option.
#
# modules/nucleus is the contract, not the floor: imported unconditionally on
# every host as an extra module, it declares the options (and the assertions
# over them) that dendrites and consumers read, and configures nothing. What a
# host carries and may deselect — a service, a package, a setting — is a
# dendrite that is a member of the `nucleus` aggregation instead
# (`example-dendrite.nix`, then one member line in
# `modules/aggregations/nucleus/default.nix`).
#
# This is a plain NixOS module, evaluated in the platform pass, so it can
# neither read nor influence selection.
{ lib, ... }:
{
  options.aoide.example = {
    greeting = lib.mkOption {
      type = lib.types.str;
      default = "hello";
      description = "What the example dendrite says; declared here so any module may read it.";
    };
  };
}
