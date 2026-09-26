# modules/dendrites/mcfly.nix — mcfly smart shell history.
#
# Dendrite shape v1 (CONTRACTS.md §2):
#   - Guarded on aoide.mcfly.enable (default false — shipped but off).
#   - Carries its own dependencies; reads no other module.
#   - Enable with one line in hosts/ (the `base` aggregation defaults it on —
#     see modules/aggregations/base/default.nix).
#
# What this dendrite does (ported verbatim from dxflake modules/dendrites/
# mcfly.nix): programs.mcfly with vim key-scheme and fuzzy search. Bash
# integration is OFF here on purpose — the bash dendrite runs `mcfly init bash`
# itself (bashrcExtra), so leaving HM's integration off avoids a double init.

let
  body =
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
    };
in
{
  inherit body;

  nixos =
    { lib, ... }:
    {
      imports = [ body ];
      config.aoide.mcfly.enable = lib.mkDefault true;
    };
}
