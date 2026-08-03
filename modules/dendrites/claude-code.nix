# modules/dendrites/claude-code.nix — the Claude Code CLI (agentic AI coding
# assistant, nixpkgs' claude-code).
#
# Dendrite shape v0 (CONTRACTS.md §2):
#   - Guarded on aoide.claude-code.enable (default false — shipped but off).
#   - Carries its own dependencies; reads no other module.
#   - Enable with one line in hosts/ (see hosts/yomi-strix/default.nix).
#
# Split out of devtools.nix (2026-08-02): a tool graduates to its own dendrite
# the moment a host needs to toggle it independently of the rest of the
# dev-tool set (same reasoning as the cli.nix/devtools.nix split). Claude Code
# remains an independently-toggleable agentic CLI rather than staying bundled
# in devtools.nix.
#
# What this dendrite does:
#   - Installs `claude` (pkgs.claude-code, unfree — the switch is scoped here,
#     narrowest-scope-wins) system-wide.
#   - Nothing else: `crc` (claude --rc) stays an alias in bash.nix, which
#     doesn't care where the binary comes from.
{
  config,
  lib,
  pkgs,
  ...
}:
{
  options.aoide.claude-code.enable = lib.mkEnableOption "the Claude Code CLI (agentic AI coding assistant)";

  config = lib.mkIf config.aoide.claude-code.enable {
    nixpkgs.config.allowUnfree = true;
    environment.systemPackages = [ pkgs.claude-code ];
  };
}
