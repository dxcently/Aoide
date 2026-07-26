# hosts/common/default.nix — cross-machine baseline.
#
# Flags only. Walked into every host by lib/mkHost.nix (it lives under
# `hosts/`, but each host `default.nix` gets it via this shared import — see
# yomi-strix). Holds the picks common to all Aoide boxes; per-host dirs add
# machine-specific overrides. `hosts/` knows dendrites; dendrites never know
# hosts.
{ config, lib, ... }:
{
  # Turn the framework on everywhere. Individual facets/dendrites still gate on
  # their own `aoide.<name>.enable` flags (flipped per host).
  aoide.enable = lib.mkDefault true;

  # MCP stays off by default — agents spawn `aoide mcp serve --stdio` per
  # session; network MCP is user-only (concepts/Agent-Interface).
  aoide.mcp.enable = lib.mkDefault false;

  # A sane baseline so `nixosSystem` can evaluate toplevel without a real host
  # having to restate these. Hosts override freely.
  system.stateVersion = lib.mkDefault "25.11";
  nixpkgs.hostPlatform = lib.mkDefault "x86_64-linux";

  # The aoide user: a normal account every facet/service hangs off (user
  # services, home-manager files, greetd session). Hosts override freely.
  users.users.${config.aoide.user} = {
    isNormalUser = lib.mkDefault true;
    extraGroups = lib.mkDefault [
      "wheel"
      "video"
      "audio"
      "networkmanager"
    ];
  };

  # The aoide user's home-manager baseline (facets write into this user's home:
  # QML tree, hyprland.conf). stateVersion pins HM's compat behaviour.
  home-manager.users.${config.aoide.user}.home.stateVersion = lib.mkDefault "25.11";

  # ── Baseline dev-tool dendrites ───────────────────────────────────────────
  # The dev-tool dendrites ported from dxflake belong on every Aoide box, so
  # they default ON here. mkDefault keeps a host free to opt any of them out
  # (a per-host `aoide.<name>.enable = false;` wins), matching how the framework
  # and MCP flags above are defaulted. Per-feature dendrites still each gate on
  # their own `aoide.<name>.enable` (CONTRACTS.md §2) — this just flips the
  # baseline. Contrast the desktop-app dendrites (e.g. obsidian) which stay OFF
  # and are opted in per host.
  aoide.bash.enable = lib.mkDefault true;
  aoide.nh.enable = lib.mkDefault true;
  aoide.git.enable = lib.mkDefault true;
  aoide.kitty.enable = lib.mkDefault true;
  aoide.neovim.enable = lib.mkDefault true;
  aoide.starship.enable = lib.mkDefault true;
  aoide.mcfly.enable = lib.mkDefault true;
  aoide.btop.enable = lib.mkDefault true;
  aoide.yazi.enable = lib.mkDefault true;
  aoide.fastfetch.enable = lib.mkDefault true;
  aoide.devtools.enable = lib.mkDefault true;
  aoide.fonts.enable = lib.mkDefault true;
}
