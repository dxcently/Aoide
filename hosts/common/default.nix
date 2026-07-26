# hosts/common/default.nix — cross-machine baseline.
#
# Flags only. Walked into every host by lib/mkHost.nix (it lives under
# `hosts/`, but each host `default.nix` gets it via this shared import — see
# yomi-strix). Holds the picks common to all Aoide boxes; per-host dirs add
# machine-specific overrides. `hosts/` knows dendrites; dendrites never know
# hosts.
{ lib, ... }:
{
  # Turn the framework on everywhere. Individual facets/dendrites still gate on
  # their own `aoide.<name>.enable` flags (flipped per host).
  aoide.enable = lib.mkDefault true;

  # MCP stays off by default — agents spawn `aoide mcp serve --stdio` per
  # session; network MCP is user-only (concepts/Agent-Interface).
  aoide.mcp.enable = lib.mkDefault false;

  # A sane baseline so `nixosSystem` can evaluate toplevel without a real host
  # having to restate these. Hosts override freely.
  system.stateVersion = lib.mkDefault "25.11";
  nixpkgs.hostPlatform = lib.mkDefault "x86_64-linux";
}
