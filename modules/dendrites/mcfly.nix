# modules/dendrites/mcfly.nix — mcfly smart shell history.
#
# Dendrite shape (CONTRACTS.md §2):
#   - Selection is the switch: no enable flag; the body runs when habit selects it.
#   - Carries its own dependencies; reads no other module.
#   - Selected by the nucleus aggregation (modules/aggregations/nucleus/default.nix), or one line in hosts/.
#
# What this dendrite does (ported verbatim from dxflake modules/dendrites/
# mcfly.nix): programs.mcfly with vim key-scheme and fuzzy search. Bash
# integration is OFF here on purpose — the bash dendrite runs `mcfly init bash`
# itself (bashrcExtra), so leaving HM's integration off avoids a double init.

{ lib, ... }:
{

  config = {
    habit.home = _: {
      programs.mcfly = {
        enable = true;
        enableBashIntegration = false;
        keyScheme = "vim";
        fuzzySearchFactor = 2;
      };
    };
  };
}
