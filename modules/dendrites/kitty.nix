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
#
# Conduct-by-default (Aoide core behaviour — concepts/Conductor-Channel): kitty's
# `shell` is pointed at the `aoide-shell` wrapper below, so EVERY kitty window
# runs its login shell under `aoide conduct` — each terminal becomes its own
# tracked, conductable session (a control socket a central agent can type into,
# nested into the DAG when spawned from another session). The wrapper is written
# to be UNBREAKABLE: any failure to conduct falls back to the plain login shell,
# and `AOIDE_NO_CONDUCT=1` is the explicit escape hatch.
{ config, lib, ... }:
{
  options.aoide.kitty.enable = lib.mkEnableOption "the Kitty terminal (colours/fonts deferred to the Stylix facet)";

  config = lib.mkIf config.aoide.kitty.enable {
    home-manager.users.${config.aoide.user} =
      { pkgs, lib, ... }:
      let
        # aoide-shell — kitty's login shell: conduct-by-default with a hard
        # safe-fallback. Order matters and every branch ends in an `exec` so a
        # broken conduct can NEVER strand the user without a shell:
        #   1. AOIDE_NO_CONDUCT set        → the plain login shell (escape hatch).
        #   2. `aoide` not on PATH         → the plain login shell (never shell-less).
        #   3. otherwise                   → `aoide conduct` the login shell, always
        #      parented to $AOIDE_SESSION_ID when already in the env (nested
        #      terminals build the DAG tree). We ALWAYS wrap — a kitty spawned from
        #      a conducted shell is still its own conducted session, just parented.
        #   4. belt-and-suspenders         → if the conduct exec ever returns, fall
        #      through to the plain login shell anyway.
        # The login shell is resolved from $SHELL, then passwd, then /bin/sh.
        aoide-shell = pkgs.writeShellScriptBin "aoide-shell" ''
          # Resolve the user's login shell robustly.
          login_shell="''${SHELL:-}"
          if [ -z "$login_shell" ] || [ ! -x "$login_shell" ]; then
            login_shell="$(getent passwd "$(id -u)" 2>/dev/null | cut -d: -f7)"
          fi
          if [ -z "$login_shell" ] || [ ! -x "$login_shell" ]; then
            login_shell=/bin/sh
          fi

          # (1) Escape hatch: never conduct when explicitly opted out.
          if [ -n "''${AOIDE_NO_CONDUCT:-}" ]; then
            exec "$login_shell" -l
          fi

          # (2) Never leave the user shell-less: only conduct if aoide is present.
          if ! command -v aoide >/dev/null 2>&1; then
            exec "$login_shell" -l
          fi

          # (3) Conduct this terminal as its own tracked, conductable session.
          #     Parent it to the spawning session when one is already in the env.
          if [ -n "''${AOIDE_SESSION_ID:-}" ]; then
            exec aoide conduct --agent shell --parent "$AOIDE_SESSION_ID" -- "$login_shell" -l
          else
            exec aoide conduct --agent shell -- "$login_shell" -l
          fi

          # (4) Belt-and-suspenders: conduct failed to exec — fall back cleanly.
          exec "$login_shell" -l
        '';
      in
      {
        programs.kitty = lib.mkForce {
          enable = true;
          package = pkgs.kitty;
          # font.name / font.size and colours are set by the Stylix facet.
          settings = {
            # Conduct-by-default: every window's shell is the wrapper above.
            shell = "${aoide-shell}/bin/aoide-shell";
            scrollback_lines = 2000;
            wheel_scroll_min_lines = 1;
            confirm_os_window_close = 0;
            window_padding_width = 5;
            window_border_width = 1.5;
            # Aero-glass terminal: a translucent background so the compositor's
            # blur reads through as frosted glass (the Win7-style sheen), while
            # the TEXT stays fully opaque and crisp (background_opacity fades only
            # the cell background, not the glyphs). Hyprland owns the blur pass —
            # it blurs behind any translucent surface when decoration:blur is on
            # (compositor facet, global) — so kitty's own background_blur (a
            # macOS/KDE-only path, inert under Hyprland) is turned off here and
            # the compositor does the frosting instead. The kitty window class is
            # additionally pinned in the compositor's Aero window rules.
            background_opacity = "0.60";
            background_blur = 0;
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
