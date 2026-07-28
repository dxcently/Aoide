# modules/dendrites/fastfetch/default.nix — the fastfetch greeting.
#
# A DIRECTORY dendrite because it carries an asset (./ascii-fetch, the logo:
# Aoide's lyre in compact form — three strings, curved arms, a soundbox, the
# A·O·I·D·E ground and a "the song" tag). Redrawn small (13×7) so the info
# column sits flush beside it and never wraps in a tiled/narrow terminal.
# The walker registers every .nix under modules/dendrites/, so this default.nix
# self-registers exactly like a flat dendrite (CONTRACTS.md §2).
#
# Dendrite shape v0:
#   - Guarded on aoide.fastfetch.enable (default false — shipped but off).
#   - Carries its own dependencies (including the bundled logo); reads no other
#     module.
#   - Enable with one line in hosts/ (see hosts/common/default.nix).
#
# Design: a clean, system-fitting greeting — a compact lyre, aligned key
# columns, and two subtly music-marked section rules (hardware / software).
# The bash dendrite calls `fastfetch` on every interactive shell (initExtra),
# so enabling both gives the login greeting.
{ config, lib, ... }:
{
  options.aoide.fastfetch.enable = lib.mkEnableOption "the fastfetch greeting (compact Aoide lyre)";

  config = lib.mkIf config.aoide.fastfetch.enable {
    home-manager.users.${config.aoide.user} =
      { pkgs, ... }:
      {
        programs.fastfetch = {
          enable = true;
          package = pkgs.fastfetch;
          settings = {
            logo = {
              type = "file";
              source = ./ascii-fetch;
              width = 14;
              height = 7;
              padding = {
                top = 1;
                left = 2;
                right = 3;
              };
            };
            display = {
              separator = "  ";
            };
            modules = [
              {
                type = "title";
                key = "♪ ";
                format = "{user-name}@{host-name}";
              }
              {
                type = "custom";
                format = "╶─────────────────────────╴";
              }
              {
                type = "os";
                key = "os    ";
              }
              {
                type = "kernel";
                key = "kernel";
              }
              {
                type = "uptime";
                key = "uptime";
              }
              { type = "break"; }
              {
                type = "custom";
                # Section rule with a subtle music mark (kept short so it never
                # runs past a tiled terminal's edge).
                format = "♪ hardware ╶──────────────╴";
              }
              {
                type = "host";
                key = "host  ";
              }
              {
                type = "cpu";
                key = "cpu   ";
                format = "{name}";
              }
              {
                type = "gpu";
                key = "gpu   ";
                format = "{name}";
              }
              {
                type = "memory";
                key = "ram   ";
              }
              { type = "break"; }
              {
                type = "custom";
                format = "♪ software ╶──────────────╴";
              }
              {
                type = "wm";
                key = "wm    ";
              }
              {
                type = "shell";
                key = "shell ";
              }
              {
                type = "terminal";
                key = "term  ";
              }
              {
                type = "theme";
                key = "theme ";
              }
              { type = "break"; }
              {
                type = "colors";
                symbol = "circle";
              }
            ];
          };
        };
      };
  };
}
