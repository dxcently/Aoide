# modules/dendrites/pi-coding-agent.nix — the Pi Coding Agent CLI
# (earendil-works/pi, nixpkgs' pi-coding-agent).
#
# Dendrite shape v0 (CONTRACTS.md §2):
#   - Guarded on aoide.pi-coding-agent.enable (default false — shipped but off).
#   - Carries its own dependencies; reads no other module.
#   - Enable with one line in hosts/ (see hosts/yomi-strix/default.nix).
#
# Third sibling of claude-code.nix / kimi-code.nix: another agentic terminal
# coding CLI, each toggled independently rather than bundled into
# devtools.nix (same reasoning as that split). Pi is also the structural
# reference for aoide's own crate-per-charter package restructure
# (docs/architecture/PACKAGE-LAYOUT.md) — packaging its CLI here is
# unrelated to that: this just puts the binary on PATH, MIT-licensed,
# straight from nixpkgs (no unfree flag, unlike claude-code).
#
# What this dendrite does:
#   - Installs `pi` (pkgs.pi-coding-agent) system-wide. Nothing else — no
#     systemd unit, no adapter wiring (unlike melete.nix/mneme.nix, which run
#     daemons this repo dispatches jobs to). Auth/config is out of scope here.
{
  config,
  lib,
  pkgs,
  ...
}:
{
  options.aoide.pi-coding-agent.enable = lib.mkEnableOption "the Pi Coding Agent CLI (earendil-works/pi)";

  config = lib.mkIf config.aoide.pi-coding-agent.enable {
    environment.systemPackages = [ pkgs.pi-coding-agent ];
  };
}
