# modules/dendrites/claude-code.nix — the Claude Code CLI (agentic AI coding
# assistant, pkgs/claude-code: nixpkgs' recipe pinned ahead).
#
# Dendrite shape (CONTRACTS.md §2):
#   - Selection is the switch: no enable flag; the body runs when habit selects it.
#   - Carries its own dependencies; reads no other module.
#   - Select with one line in hosts/ (see hosts/yomi-strix/default.nix).
#
# Split out of devtools.nix (2026-08-02): a tool graduates to its own dendrite
# the moment a host needs to toggle it independently of the rest of the
# dev-tool set (same reasoning as the cli.nix/devtools.nix split). Kimi Code
# (kimi-code.nix) is its sibling agentic CLI and got the same per-tool
# toggle, so claude-code got one too rather than staying bundled in
# devtools.nix.
#
# What this dendrite does:
#   - Installs `claude` (pkgs.claude-code, unfree — the switch is scoped here,
#     narrowest-scope-wins) system-wide.
#   - Nothing else: `crc` (claude --rc) stays an alias in bash.nix, which
#     doesn't care where the binary comes from.

{
  lib,
  pkgs,
  ...
}:
{

  config = {
    nixpkgs.config.allowUnfree = true;
    environment.systemPackages = [ pkgs.claude-code ];
  };
}
