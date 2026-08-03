# modules/dendrites/kimi-code.nix — Kimi Code CLI (Moonshot AI's terminal
# coding agent, github.com/MoonshotAI/kimi-code).
#
# Dendrite shape v0 (CONTRACTS.md §2):
#   - Guarded on aoide.kimi-code.enable (default false — shipped but off).
#   - Carries its own dependencies (pkgs/kimi-code); reads no other module.
#   - Enable with one line in hosts/ (see hosts/yomi-strix/default.nix).
#
# What this dendrite does:
#   - Installs `kimi` (pkgs.kimi-code — an autoPatchelf'd upstream prebuilt
#     binary, MIT-licensed, no unfree flag needed) system-wide.
#   - Nothing else: it's a standalone terminal agent like claude-code, not a
#     background service — no systemd unit, no adapter wiring (unlike
#     melete.nix/mneme.nix, which run daemons this repo dispatches jobs to).
#     Auth is interactive (`kimi` → `/login`) or via its own env/config, both
#     out of this module's scope.
{
  config,
  lib,
  pkgs,
  ...
}:
{
  options.aoide.kimi-code.enable = lib.mkEnableOption "the Kimi Code CLI (Moonshot AI's terminal coding agent)";

  config = lib.mkIf config.aoide.kimi-code.enable {
    environment.systemPackages = [ pkgs.kimi-code ];
  };
}
