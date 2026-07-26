# modules/dendrites/yazi.nix — the yazi terminal file manager.
#
# Dendrite shape v0 (CONTRACTS.md §2):
#   - Guarded on aoide.yazi.enable (default false — shipped but off).
#   - Carries its own dependencies; reads no other module.
#   - Enable with one line in hosts/ (see hosts/common/default.nix).
#
# What this dendrite does (ported verbatim from dxflake modules/dendrites/
# yazi.nix): programs.yazi with the dxflake layout/sort settings, a `y` shell
# wrapper (bash integration; the bash dendrite also defines a `y` cd-wrapper —
# they name the same tool), and a `sy` = "sudo yazi" alias.
{ config, lib, ... }:
{
  options.aoide.yazi.enable = lib.mkEnableOption "the yazi terminal file manager";

  config = lib.mkIf config.aoide.yazi.enable {
    home-manager.users.${config.aoide.user} =
      { ... }:
      {
        programs.yazi = {
          enable = true;
          enableBashIntegration = true;
          shellWrapperName = "y";

          settings = {
            manager = {
              ratio = [
                0
                1
                1
              ];
              sort_by = "mtime";
              sort_sensitive = false;
              sort_reverse = true;
              linemode = "size";
              show_hidden = false;
            };
          };

          theme = {
            mgr = {
              preview_hovered = {
                underline = false;
              };
              folder_offset = [
                1
                0
                1
                0
              ];
              preview_offset = [
                1
                1
                1
                1
              ];
            };

            status.separator_style = {
              fg = "red";
              bg = "red";
            };
          };
        };

        home.shellAliases = {
          sy = "sudo yazi";
        };
      };
  };
}
