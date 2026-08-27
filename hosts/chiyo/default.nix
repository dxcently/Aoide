# hosts/chiyo/default.nix — the first lyra carrier.
#
# yomi-strix is a desktop tower; chiyo is a laptop, and the first host to run
# the full paint stack (facets/quickshell/lyra) on portable hardware rather
# than a fixed venue. Same shape as yomi-strix (flags + a guarded hardware
# import, no module files imported directly) minus yomi's venue-specific
# hardware tuning (Strix Halo's amdgpu kernel params, the 24 GiB GTT carve,
# zram sized for a 32 GB pool) — chiyo states only what its own Intel/laptop
# hardware needs.
#
# chiyo mostly lives on public/corporate wifi that blocks WireGuard/tailscale
# outright; the reachable network path is a Cloudflare Tunnel (ssh only),
# managed in the operator's deployment flake, not here.
{ lib, pkgs, ... }:
{
  imports = [
    ../common
  ]
  # Guarded: import ./hardware.nix only if the file exists, so the flake
  # still evaluates on a machine without a committed hardware scan.
  ++ lib.optional (builtins.pathExists ./hardware.nix) ./hardware.nix;

  networking.hostName = "chiyo";
  networking.networkmanager.enable = true;

  time.timeZone = "America/New_York";

  # Portable venue: stock NixOS power management (not an aoide flag) plus the
  # generic graphics stack (Intel iGPU needs no vendor-specific driver list
  # or kernel params the way yomi-strix's amdgpu does). No display/monitor
  # layout here on purpose — a laptop's panel is discovered, not pinned.
  powerManagement.enable = true;
  hardware.graphics.enable = true;

  # Aoide flags for this box.
  aoide.enable = true;
  aoide.user = "khoa";
  aoide.song = "sonata";

  # The whole desktop, one line each — same set yomi-strix runs, because
  # chiyo is the first laptop meant to carry the full paint stack, not a
  # trimmed-down profile.
  aoide.facets.quickshell.enable = true;
  aoide.facets.compositor.enable = true;
  aoide.facets.stylix.enable = true;

  # Host-invariant Hyprland behaviour (keybinds, input, layout, window rules).
  aoide.hyprland.enable = true;

  # Screen capture, clipboard, notifications, audio, NM applet — the desktop
  # conveniences yomi-strix runs, unchanged: none of these carry a
  # yomi-specific assumption.
  aoide.screenshot.enable = true;
  aoide.vision.enable = true;
  aoide.clipboard.enable = true;
  aoide.dunst.enable = true;
  aoide.audio.enable = true;
  aoide.networkmanager.enable = true;

  # A2A door — enabled with a live spawn target, matching the dxflake
  # deployment pattern (sakaki): spawnAgent names the command, spawnPath
  # carries its package onto the systemd user unit's PATH so a bare `claude`
  # resolves. Loopback-bound (bindAddress default); chiyo's Cloudflare
  # tunnel forwards ssh only, never this port, so the loopback trust model
  # yomi-strix relies on still holds. discoveryAdvertise stays off (default)
  # — chiyo's usual networks are public/corporate wifi, not a trusted LAN to
  # broadcast a beacon on.
  aoide.a2a.enable = true;
  aoide.a2a.spawnAgent = "claude";
  aoide.a2a.spawnPath = [ pkgs.claude-code ];

  # Secrets broker (workstream #58, P-V4 deployment): own uid, socket-only
  # door. The operator joins the access group; enrollment happens only when
  # the User says connect.
  aoide.secrets.enable = true;
  aoide.secrets.members = [ "khoa" ];

  # Shipped dendrites this box actually uses (mirrors what chiyo already ran
  # under dxflake). Obsidian is left off on purpose: the wiki vault this
  # dendrite would serve lives in the cloned Aoide repo and is reachable
  # through Mneme already — add the desktop app back if the operator wants
  # it on-device too.
  aoide.firefox.enable = true;
  aoide.claude-code.enable = true;
  aoide.kimi-code.enable = true;
  aoide.pi-coding-agent.enable = true;
}
