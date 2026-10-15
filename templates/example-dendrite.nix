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
# module is. Selection is the switch: this file is imported only when something
# selected it, so its body runs unconditionally. Nothing here gates on an
# `enable` flag of its own, and nothing sets one. A fact (a lane's
# `aoide.<name>.enable` that another lane must read) is declared in
# modules/nucleus/options.nix only, and is then read-only and set only by its
# owner; a capability nobody reads declares nothing.
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
# A condition around `habit.home` carries down to what the half sets. It cannot
# cover the half's `imports` or `options`, which are read before any condition
# is: put those in a `habit.home` of their own, outside it.
#
# This file is imported only if something selected it. Nothing here runs at
# selection time.
_: {
  # Carry your own dependencies. This is where a real capability puts its
  # packages and its services — `environment.systemPackages = [ pkgs.jq ];`,
  # `services.example.enable = true;` — and it is live as soon as the dendrite
  # is selected.
  #
  # Read no other module — only the options this file declares, plus stock
  # NixOS options.

  # Optional. Drop this for a system-only capability.
  habit.home =
    { pkgs, ... }:
    {
      home.packages = [ pkgs.jq ];
    };
}
