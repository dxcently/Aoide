# modules/dendrites/_example.nix
#
# SHELVED — this file is intentionally IGNORED by the walker (lib/walk.nix).
# Any path containing "/_" is skipped by the walker's filter, so prefixing
# a file or directory with "_" hides it from discovery without deleting it.
# Use this convention for work-in-progress dendrites, templates, and scratch
# files. Remove the leading "_" when the dendrite is ready to activate.
#
# ── Example dendrite shape (v0, CONTRACTS.md §2) ──────────────────────────
#
# { config, lib, ... }:
# {
#   options.aoide.<name>.enable = lib.mkEnableOption "<name>";
#   config = lib.mkIf config.aoide.<name>.enable {
#     # Carry your own dependencies (narrowest scope wins).
#     # Read no other module — only config.aoide.* options you declare yourself,
#     # plus stock NixOS options.
#   };
# }
#
# Enable with one line in hosts/yomi-strix/default.nix:
#   aoide.<name>.enable = true;
#
# hosts/ knows dendrites; dendrites never know hosts.
{ lib, ... }:
{
  # This module intentionally declares nothing — it is a documentation stub.
  # Because of the "/_" path prefix, it will never be loaded by the walker
  # even if left in this file location.
}
