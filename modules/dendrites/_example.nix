# modules/dendrites/_example.nix
#
# SHELVED — a `_`-prefixed file or directory is not a module: nothing imports
# it and the catalogue does not name it. Keep it in place as a template or a
# work-in-progress; prefixing is the opt-out, so nothing here has to be deleted.
#
# ── Example dendrite shape (CONTRACTS.md §2) ───────────────────────────────
#
# A dendrite is a PLAIN MODULE: it declares the options and guards its config,
# and sets the flag `mkDefault true`, so that selecting the dendrite is what
# turns the capability on. What belongs in the user's home goes in
# `habit.home`, which the constructor splits off and hands to Home Manager.
#
# One line activates it, in the catalogue — the same line that makes it
# selectable is the only place its file is named:
#   modules/default.nix             example = ./dendrites/example.nix;
#
# hosts/ knows dendrites; dendrites never know hosts.
{ config, lib, ... }:
{
  options.aoide.example.enable = lib.mkEnableOption "the example capability";

  config = lib.mkMerge [
    { aoide.example.enable = lib.mkDefault true; }
    (lib.mkIf config.aoide.example.enable {
      # Carry your own dependencies (narrowest scope wins).
      # Read no other module — only config.aoide.example.* options declared
      # here, plus stock NixOS options.
      #
      # habit.home = { programs.example.enable = true; };
    })
  ];
}
