# modules/dendrites/mcfly.nix — mcfly smart shell history.
#
# Dendrite shape v0 (CONTRACTS.md §2):
#   - Guarded on aoide.mcfly.enable (default false — shipped but off).
#   - Carries its own dependencies; reads no other module.
#   - Enable with one line in hosts/ (see hosts/common/default.nix).
#
# What this dendrite does (ported verbatim from dxflake modules/dendrites/
# mcfly.nix): programs.mcfly with vim key-scheme and fuzzy search. Bash
# integration is OFF here on purpose — the bash dendrite runs `mcfly init bash`
# itself (bashrcExtra), so leaving HM's integration off avoids a double init.
{ config, lib, ... }:
{
  options.aoide.mcfly.enable = lib.mkEnableOption "mcfly smart shell history (vim keys, fuzzy search)";

  config = lib.mkIf config.aoide.mcfly.enable {
    home-manager.users.${config.aoide.user} = _: {
      programs.mcfly = {
        enable = true;
        enableBashIntegration = false;
        keyScheme = "vim";
        fuzzySearchFactor = 2;
      };
    };
  };
}
