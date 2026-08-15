# hosts/_desktop/default.nix — TEMPLATE: desktop workstation skeleton.
#
# Shelved (the `_` prefix): not registered in flake.nix, so nothing builds or
# evals it. yomi-strix stays the living reference; this is the generic starting
# shape. To adopt:
#   1. cp -r hosts/_desktop hosts/<your-hostname>
#   2. flake.nix: `nixosConfigurations.<your-hostname> = mkHost "<your-hostname>";`
#   3. set hostName/aoide.user/timeZone below; drop a committed hardware.nix
#      next to this file (the guarded import tolerates its absence)
#   4. nixos-rebuild switch --flake .#<your-hostname>
{ lib, pkgs, ... }:
{
  imports = [
    ../common
  ]
  # Guarded: import ./hardware.nix only if the file exists, so the flake
  # still evaluates on a machine without a committed hardware scan.
  ++ lib.optional (builtins.pathExists ./hardware.nix) ./hardware.nix;

  networking.hostName = "desktop"; # ← your hostname
  networking.networkmanager.enable = true;

  time.timeZone = "UTC"; # ← your zone

  # Venue graphics — pick your driver and any venue kernel params here.
  # (yomi-strix shows the amdgpu + latest-kernel version of this block.)
  hardware.graphics.enable = true;

  # Aoide flags. hosts/common already defaults the framework + baseline
  # dendrites ON; a host only flips what differs.
  aoide.enable = true;
  aoide.user = "khoa"; # ← your user

  # The song this host performs (sonata is the shipped standard default;
  # stated explicitly as good practice). Replay any committed
  # song/songbook/<name>/ with this one line.
  aoide.song = "sonata";

  # The whole desktop, one line each (facets render; hyprland owns the
  # rice-invariant behaviour — keybinds, input, window rules):
  aoide.facets.quickshell.enable = true;
  aoide.facets.compositor.enable = true;
  aoide.facets.stylix.enable = true;
  aoide.hyprland.enable = true;

  # Desktop conveniences:
  aoide.screenshot.enable = true; # hyprshot+satty for the human
  aoide.vision.enable = true; # grim/slurp for agents
  aoide.clipboard.enable = true; # cliphist history + QML picker
  aoide.audio.enable = true; # PipeWire + WirePlumber
  aoide.networkmanager.enable = true; # nm-connection-editor + nm-applet

  # Opt-in dendrites (browse modules/dendrites/; aoide.mcp.enable stays false
  # — house policy):
  aoide.firefox.enable = true;
  aoide.claude-code.enable = true;
}
