# modules/dendrites/starship.nix — the Starship prompt.
#
# Dendrite shape v0 (CONTRACTS.md §2):
#   - Guarded on aoide.starship.enable (default false — shipped but off).
#   - Carries its own dependencies; reads no other module.
#   - Enable with one line in hosts/ (see hosts/common/default.nix).
#
# What this dendrite does (ported verbatim from dxflake modules/dendrites/
# starship.nix): the musical-notation Starship prompt — clef-decorated two-line
# format, git branch/status with staff glyphs, ♪/𝄽 success/error character.
# Bash integration on (pairs with the bash dendrite).
{ config, lib, ... }:
{
  options.aoide.starship.enable = lib.mkEnableOption "the Starship prompt (musical-notation theme)";

  config = lib.mkIf config.aoide.starship.enable {
    home-manager.users.${config.aoide.user} =
      { ... }:
      {
        programs.starship = {
          enable = true;
          enableBashIntegration = true;
          settings = {
            add_newline = true;

            format = ''
              ═𝄞═══ $directory𝅘𝅥𝅮 $git_branch$git_status
              ═𓏲𝄢═══ $username@$hostname$character'';

            directory = {
              truncation_length = 3;
              truncate_to_repo = false;
              read_only = " ♯";
            };

            username = {
              show_always = true;
              format = "[$user]($style)";
            };

            hostname = {
              ssh_only = false;
              format = "[$hostname]($style) ";
            };

            git_branch = {
              symbol = "♬ ";
            };

            git_status = {
              format = "[♭$all_status$ahead_behind]($style) ";
              ahead = "𝄪\${count}";
              behind = "𝄫\${count}";
              modified = "𝅗𝅥";
              staged = "𝅘𝅥";
              untracked = "𝅝";
              conflicted = "𝄢";
            };

            character = {
              success_symbol = "♪";
              error_symbol = "𝄽";
            };
          };
        };
      };
  };
}
