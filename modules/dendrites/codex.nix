# modules/dendrites/codex.nix — the OpenAI Codex CLI (openai/codex, nixpkgs'
# codex).
#
# Dendrite shape v0 (CONTRACTS.md §2):
#   - Guarded on aoide.codex.enable (default false — shipped but off).
#   - Carries its own dependencies; reads no other module.
#   - Enable with one line in hosts/ (see hosts/yomi-strix/default.nix).
#
# Fourth sibling of claude-code.nix / kimi-code.nix / pi-coding-agent.nix:
# another agentic terminal CLI, toggled per-tool rather than bundled into
# devtools.nix (same reasoning as that split). Apache-2.0 from nixpkgs, so no
# unfree flag (unlike claude-code) and no vendored package (unlike kimi-code).
#
# What this dendrite does:
#   - Installs `codex` (pkgs.codex) system-wide.
#   - Nothing else. Codex is the wiki's canonical HOOKLESS harness
#     (concepts/orchestration/Agent-Hooking.md §3): it carries no
#     `AgentProfile` row and no hook-settings install, and reaches the session
#     graph through the wrapper instead — `aoide conduct --agent codex --
#     codex …` registers it, mirrors its exit, and leaves it steerable by
#     `aoide send`, none of which needs the harness's cooperation. Auth is
#     interactive (`codex login`) or its own env/config, out of scope here as
#     in every sibling.
{
  config,
  lib,
  pkgs,
  ...
}:
{
  options.aoide.codex.enable = lib.mkEnableOption "the OpenAI Codex CLI (openai/codex)";

  config = lib.mkIf config.aoide.codex.enable {
    environment.systemPackages = [ pkgs.codex ];
  };
}
