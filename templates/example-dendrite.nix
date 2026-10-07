# example-dendrite.nix — a capability with ONE implementation.
#
# Copy to:  modules/dendrites/<capability>.nix
# Then:     add `<capability> = ./dendrites/<capability>.nix;` to the catalogue
#           in modules/default.nix, and select it from a host, a user, or an
#           aggregation's `members`. That line is the only place this file is
#           named.
# Replace:  <capability>, and everything under `config`.
#
# A dendrite is a PLAIN MODULE: an ordinary NixOS module, written the way any
# module is. It declares `aoide.<capability>.*` and guards its config with
# `aoide.<capability>.enable`, so the option, its guard and the capability's own
# config stay together; and it sets the flag `lib.mkDefault true`, so selecting
# the dendrite is what turns the capability on.
#
# habit splits a selected module into two halves:
#
#   system  the module as written, minus `habit` — services, system packages,
#           hardware. Applied in the host's evaluation.
#   home    the value of `habit.home` — dotfiles, user packages. A module of
#           Home Manager's, handed to each user the module reaches.
#
# A module with no `habit.home` has an empty home half, and one with only
# `habit.home` has an empty system half. Selecting a dendrite for the host
# reaches every user with `home.enable = true`; selecting it under `users.<u>`
# reaches that user alone (and applies its system half as well).
#
# A condition around `habit.home` — the `mkIf` below — carries down to what the
# half sets. It cannot cover the half's `imports` or `options`, which are read
# before any condition is: put those in a `habit.home` of their own, outside it.
#
# This file is imported only if something selected it. Nothing here runs at
# selection time.
{ config, lib, ... }:
{
  options.aoide.example.enable = lib.mkEnableOption "the example capability";

  config = lib.mkMerge [
    { aoide.example.enable = lib.mkDefault true; }
    (lib.mkIf config.aoide.example.enable {
      # Carry your own dependencies (narrowest scope wins). This is where a
      # real capability puts its packages and its services —
      # `environment.systemPackages = [ pkgs.jq ];`, `services.example.enable
      # = true;` — and it stays inert until the flag is on.
      #
      # Read no other module — only config.aoide.example.* options declared
      # here, plus stock NixOS options.

      # Optional. Drop this for a system-only capability.
      habit.home =
        { pkgs, ... }:
        {
          home.packages = [ pkgs.jq ];
        };
    })
  ];
}
