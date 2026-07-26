# modules/dendrites/kitty.nix — the Kitty terminal.
#
# Dendrite shape v0 (CONTRACTS.md §2):
#   - Guarded on aoide.kitty.enable (default false — shipped but off).
#   - Carries its own dependencies; reads no other module.
#   - Enable with one line in hosts/ (see hosts/common/default.nix).
#
# What this dendrite does (ported from dxflake modules/dendrites/kitty.nix):
#   - programs.kitty with the dxflake settings (scrollback, padding, powerline
#     tab bar, no audio bell) and its alt-key window/tab keybindings.
#   - Font/colours are left to the Stylix facet (aoide.facets.stylix), exactly
#     as dxflake left them to its stylix layer — so no colours are hard-coded
#     here.
#
# Adapted vs dxflake: dxflake gated this on `dx.aggregations.desktop`; Aoide has
# no aggregation flags, so it gates on its own aoide.kitty.enable per §2. The
# `lib.mkForce` dxflake used to win over its stylix module's kitty defaults is
# kept so the terminal is a clean single definition.
{ config, lib, ... }:
{
  options.aoide.kitty.enable = lib.mkEnableOption "the Kitty terminal (colours/fonts deferred to the Stylix facet)";

  config = lib.mkIf config.aoide.kitty.enable {
    home-manager.users.${config.aoide.user} =
      { pkgs, lib, ... }:
      {
        programs.kitty = lib.mkForce {
          enable = true;
          package = pkgs.kitty;
          # font.name / font.size and colours are set by the Stylix facet.
          settings = {
            scrollback_lines = 2000;
            wheel_scroll_min_lines = 1;
            confirm_os_window_close = 0;
            window_padding_width = 5;
            window_border_width = 1.5;
            background_opacity = 1;
            background_blur = 1;
            enable_audio_bell = false;
            tab_bar_style = "powerline";
            tab_powerline_style = "slanted";
          };
          keybindings = {
            "alt+j" = "next_window";
            "alt+k" = "previous_window";
            "alt+h" = "previous_tab";
            "alt+l" = "next_tab";
            "alt+enter" = "new_window_with_cwd";
            "alt+shift+t" = "new_tab_with_cwd";
            "alt+q" = "close_window";
            "ctrl+shift+U" = "none"; # for vim's page up
          };
        };
      };
  };
}
