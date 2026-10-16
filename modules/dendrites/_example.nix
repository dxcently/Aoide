# modules/dendrites/_example.nix
#
# SHELVED — a `_`-prefixed file or directory is not a module: nothing imports
# it and the catalogue does not name it. Keep it in place as a template or a
# work-in-progress; prefixing is the opt-out, so nothing here has to be deleted.
#
# ── Example dendrite shape (CONTRACTS.md §2) ───────────────────────────────
#
# A dendrite is a PLAIN MODULE. Selection is the switch: this file is imported
# only when something selected it, so its body runs unconditionally. What
# belongs in the user's home goes in `habit.home`, which the constructor splits
# off and hands to Home Manager.
#
# One line activates it, in the catalogue — the same line that makes it
# selectable is the only place its file is named:
#   modules/default.nix             example = ./dendrites/example.nix;
#
# hosts/ knows dendrites; dendrites never know hosts.
_: {
  # Carry your own dependencies (narrowest scope wins). Read no other module —
  # only options this file declares itself, plus stock NixOS options.
  #
  # habit.home = { programs.example.enable = true; };
}
