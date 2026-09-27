# hosts/_mac/default.nix — TEMPLATE (forward-looking): macOS / nix-darwin host.
#
# Shelved (the `_` prefix): not discovered by flake.nix, so nothing builds or
# evaluates it. NOT EVALUABLE TODAY — the constructor is nixosSystem-only.
# Landing a mac host needs the class seam first:
#   1. flake.nix: a nix-darwin input (follows nixpkgs), `aarch64-darwin` in
#      `systems`, a `darwinConfigurations` output
#   2. lib/composition.nix / lib/aoideos.nix: a `class` arg — darwinSystem plus
#      home-manager's and stylix's *darwinModules* instead of the nixosModules
#      sets (`composition.laneNames` already carries `darwin`)
#   3. modules/nucleus: a launchd twin for the aoided/shellbridge user services
#      (systemd.user.services → launchd.user.agents); `nixpkgs.hostPlatform`
#      and `users.users.*.isNormalUser` are NixOS-only options
#      that need a class split
#   4. pkgs/aoide/flake.nix: add aarch64-darwin to its systems
# See docs/architecture/PACKAGE-LAYOUT.md — "NixOS optional, never required."
#
# The shape once that lands — Aoide core only (the paint lanes are Linux:
# Hyprland); songs still RESOLVE (a rice.nix only sets aoide.livery.*, our own
# option namespace) but nothing renders them on macOS. NOTE: no
# `imports = [ ../common ]` here on purpose — common carries NixOS-only
# options (and defaults the blocked-off dendrites below ON), so until it gets
# its class split this template stands alone and restates what it wants:
{ lib, ... }:
{
  networking.hostName = "mac"; # ← your hostname
  nixpkgs.hostPlatform = "aarch64-darwin";

  aoide.enable = true;
  aoide.user = "khoa"; # ← your user

  # ── Darwin-incompatible baseline: blocked off here, host-side ─────────────
  # The baseline aggregations default these ON; each carries NixOS-only wiring or a
  # package that doesn't belong on macOS, so the template opts out explicitly
  # (no class machinery — the host just says no):
  aoide.fonts.enable = false; # fonts.packages is a NixOS-only option
  aoide.kitty.enable = false; # GUI terminal; nixpkgs kitty on darwin is a sore spot

  # Also Linux/desktop-only — off by default, KEEP them off here:
  #   aoide.audio       (services.pipewire)        aoide.clipboard (systemd service)
  #   aoide.screenshot  (hyprshot/grim, wayland)   aoide.vision    (grim/slurp)
  #   aoide.obsidian    (systemd service)          aoide.firefox   (broken on nixpkgs-darwin)
  #   aoide.hyprland + every paint lane            (the AoideOS desktop is Linux)

  # No paint lane — the desktop is AoideOS-on-Linux. What a mac host runs:
  # aoided, the session graph, conduct, agent hooks — the headless profile,
  # same as _server.
  #
  # aoide.melete.enable = true;
  # aoide.mneme.enable = true;
}
