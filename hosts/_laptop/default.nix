# hosts/_laptop/default.nix — TEMPLATE: laptop skeleton.
#
# Shelved (the `_` prefix): not registered in flake.nix. Same shape as
# _desktop, plus the portable-venue bits (power, lid, battery — the bar's
# battery gauge reads these). To adopt:
#   1. cp -r hosts/_laptop hosts/<your-hostname>
#   2. flake.nix: `nixosConfigurations.<your-hostname> = mkHost "<your-hostname>";`
#   3. set hostName/aoide.user/timeZone; commit a hardware.nix if you have one
#   4. nixos-rebuild switch --flake .#<your-hostname>
{ lib, pkgs, ... }:
{
  imports = [
    ../common
  ]
  ++ lib.optional (builtins.pathExists ./hardware.nix) ./hardware.nix;

  networking.hostName = "laptop"; # ← your hostname
  networking.networkmanager.enable = true;

  time.timeZone = "UTC"; # ← your zone

  hardware.graphics.enable = true;

  # Portable venue: stock NixOS power management (not an aoide flag). Battery
  # level surfaces in the bar automatically once the desktop is up.
  powerManagement.enable = true;

  aoide.enable = true;
  aoide.user = "khoa"; # ← your user
  aoide.song = "sonata";

  # The whole desktop, one line each:
  aoide.facets.quickshell.enable = true;
  aoide.facets.compositor.enable = true;
  aoide.facets.stylix.enable = true;
  aoide.hyprland.enable = true;

  aoide.screenshot.enable = true;
  aoide.vision.enable = true;
  aoide.clipboard.enable = true;
  aoide.audio.enable = true;

  # Opt-in dendrites:
  aoide.firefox.enable = true;
  aoide.claude-code.enable = true;
}
