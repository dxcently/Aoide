# example-dendrite.nix — a capability with ONE implementation.
#
# Copy to:  modules/dendrites/<capability>.nix
# Then:     add `<capability> = ./dendrites/<capability>.nix;` to the catalogue
#           in modules/default.nix, and select it from a host, a user, or an
#           aggregation's `members`. One line: the whole-tree aggregate derives
#           its imports from the catalogue, so nothing else names this file.
# Replace:  <capability>, and everything under `body` and each lane.
#
# A dendrite is a LANE RECORD: a plain attribute set naming the module that
# declares the capability (`body`) and the LANES it answers for. A lane is a
# module for one evaluator:
#
#   nixos        the host's NixOS module — services, system packages, hardware
#   homeManager  one user's Home Manager module — dotfiles, user packages
#   darwin       nix-darwin; in the vocabulary, unused in this tree
#
# `body` is the module itself: it declares `aoide.<capability>.*` and guards its
# config with `aoide.<capability>.enable`, so the option, its guard and the
# capability's own config stay together whichever way the capability is reached.
# A lane imports `body` and sets the flag `lib.mkDefault true` — selecting a lane
# is what turns the capability on.
#
# Expose only the lanes this capability really answers for. There are no empty
# stand-ins: selecting a lane a dendrite does not expose is an error naming the
# dendrite, the scope and the lanes it does support. Selecting it for the system
# does NOT select it for a user, and the reverse — `dendrites.<name>.enable` on
# the host reaches `nixos`, the same line under `users.<u>` reaches
# `homeManager`, and a capability that wants both is selected in both places.
#
# This file is imported only if something selected it. Nothing here runs at
# selection time.
let
  body =
    { config, lib, ... }:
    {
      options.aoide.example.enable = lib.mkEnableOption "the example capability";

      config = lib.mkIf config.aoide.example.enable {
        # Carry your own dependencies (narrowest scope wins). This is where a
        # real capability puts its packages and its services —
        # `environment.systemPackages = [ pkgs.jq ];`, `services.example.enable
        # = true;` — and it stays inert until the flag is on.
        #
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

  # Optional. Drop this lane entirely for a system-only capability.
  #
  # A lane that outgrows one file moves out beside this one — `home.nix` or
  # `nixos.nix` in a directory of its own — and this file becomes
  #   { nixos = ./nixos.nix; homeManager = ./home.nix; }
  # with the dendrite catalogued as the DIRECTORY. Same contract, one file each.
  homeManager =
    { lib, ... }:
    {
      imports = [ body ];
      config.aoide.example.enable = lib.mkDefault true;
    };
}
