# modules/dendrites/_example.nix
#
# SHELVED — a `_`-prefixed file or directory is not a module: nothing imports
# it, no `default.nix` lists it and the catalogue does not name it. Keep it in
# place as a template or a work-in-progress; prefixing is the opt-out, so
# nothing here has to be deleted.
#
# ── Example dendrite shape (v1, CONTRACTS.md §2) ──────────────────────────
#
# A dendrite is a LANE RECORD: `body` is the module that declares the options
# and guards its config, `nixos` is the lane the constructor imports for a host
# that selected this dendrite, and that lane sets the flag `mkDefault true`.
# The body's args belong to the body — the record itself is plain data.
#
# Two lines activate it (both inside this directory's tree):
#   modules/default.nix             example = ./dendrites/example.nix;
#   modules/dendrites/default.nix   (import ./example.nix).body
#
# hosts/ knows dendrites; dendrites never know hosts.
let
  body =
    { config, lib, ... }:
    {
      options.aoide.example.enable = lib.mkEnableOption "the example capability";

      config = lib.mkIf config.aoide.example.enable {
        # Carry your own dependencies (narrowest scope wins).
        # Read no other module — only config.aoide.example.* options declared
        # here, plus stock NixOS options.
      };
    };
in
{
  inherit body;

  nixos =
    { lib, ... }:
    {
      imports = [ body ];
      config.aoide.example.enable = lib.mkDefault true;
    };
}
