# modules/dendrites/fastfetch/default.nix — the fastfetch greeting.
#
# A DIRECTORY dendrite because it carries an asset (./ascii-fetch, the logo).
# The walker registers every .nix under modules/dendrites/, so this default.nix
# self-registers exactly like a flat dendrite (CONTRACTS.md §2).
#
# Dendrite shape v0:
#   - Guarded on aoide.fastfetch.enable (default false — shipped but off).
#   - Carries its own dependencies (including the bundled logo); reads no other
#     module.
#   - Enable with one line in hosts/ (see hosts/common/default.nix).
#
# What this dendrite does (ported from dxflake modules/dendrites/fastfetch/):
#   - programs.fastfetch with the musical-notation module layout and the bundled
#     ascii logo. The bash dendrite calls `fastfetch` on every interactive
#     shell (initExtra), so enabling both gives the login greeting.
#
# Adapted vs dxflake: dxflake gated this on `dx.aggregations.desktop`; Aoide has
# no aggregation flags, so it gates on its own aoide.fastfetch.enable per §2.
{ config, lib, ... }:
{
  options.aoide.fastfetch.enable = lib.mkEnableOption "the fastfetch greeting (bundled musical ascii logo)";

  config = lib.mkIf config.aoide.fastfetch.enable {
    home-manager.users.${config.aoide.user} =
      { pkgs, ... }:
      {
        programs.fastfetch = {
          enable = true;
          package = pkgs.fastfetch;
          settings = {
            logo = {
              type = "auto";
              source = ./ascii-fetch;
              width = 39;
              height = 15;
            };
            display = {
              separator = "";
            };
            modules = [
              {
                type = "custom";
                format = "=─=─=─=─=─=─=─=─=─=─=─=─  𓏲𝄢  ─=─=─=─=─=─=─=─=─=─=─=─=";
              }
              {
                type = "title";
                key = "  𝅝 user: ";
                format = "{1}@{2}";
              }
              {
                type = "os";
                key = "  𝄞 os: ";
              }
              { type = "break"; }
              {
                type = "custom";
                format = "  hardware -------------------------------------------";
              }
              {
                type = "gpu";
                key = "  𝅘𝅥𝅯 gpu: ";
              }
              {
                type = "host";
                key = "  ♬ host: ";
              }
              {
                type = "cpu";
                key = "  ♭ cpu: ";
              }
              {
                type = "memory";
                key = "  𝄌 ram: ";
              }
              { type = "break"; }
              {
                type = "custom";
                format = "  software -------------------------------------------";
              }
              {
                type = "wm";
                key = "  𝄡 window manager:  ";
              }
              {
                type = "terminalfont";
                key = "  𝆑 font: ";
              }
              {
                type = "editor";
                key = "  ♯ editor: ";
              }
              {
                type = "terminal";
                key = "  𝅘𝅥 terminal: ";
              }
              {
                type = "shell";
                key = "  𝅗𝅥 shell: ";
              }
              {
                type = "theme";
                key = "  𝄇 color scheme: ";
              }
              {
                type = "colors";
                symbol = "square";
                paddingLeft = 21;
              }
              {
                type = "custom";
                format = "=─=─=─=─=─=─=─=─=─=─=─=─  𓏲𝄢  ─=─=─=─=─=─=─=─=─=─=─=─=";
              }
            ];
          };
        };
      };
  };
}
