# modules/dendrites/kitty.nix — the Kitty terminal.
#
# Dendrite shape (CONTRACTS.md §2):
#   - Guarded on aoide.kitty.enable (default false — shipped but off).
#   - Carries its own dependencies; reads no other module.
#   - Enable with one line in hosts/ (the `base` aggregation defaults it on —
#     see modules/aggregations/base/default.nix).
#
# What this dendrite does (ported from dxflake modules/dendrites/kitty.nix):
#   - programs.kitty with the dxflake settings (scrollback, padding, powerline
#     tab bar, no audio bell) and its alt-key window/tab keybindings.
#   - Font/colours are left to the stylix lane (aoide.stylix.enable), exactly
#     as dxflake left them to its stylix layer — so no colours are hard-coded
#     here.
#   - Staged colours (house rule 10: staging hot-loads, never waits on a
#     rebuild): kitty.conf includes `<aoide.root>/song/stage/terminal-colors.conf`
#     AFTER Stylix's baked colour include, so a new window opens in the song
#     `rice stage` last staged, and opens a per-instance control socket
#     (`listen_on`, `allow_remote_control socket-only`) over which `rice stage`
#     has the windows already open reload their config, this file included.
#     `rice mode declarative` rewrites the file from the declared twin, so
#     leaving staging restores the declared look the same live way. The path is the
#     runtime root's contract path, read off `aoide.root` like `aoide.user`.
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
# nested into the graph when spawned from another session). The wrapper is written
# to be UNBREAKABLE: any failure to conduct falls back to the plain login shell,
# and `AOIDE_NO_CONDUCT=1` is the explicit escape hatch.

{ config, lib, ... }:
{
  options.aoide.kitty.enable = lib.mkEnableOption "the Kitty terminal (colours/fonts deferred to the stylix lane)";

  config = lib.mkMerge [
    { aoide.kitty.enable = lib.mkDefault true; }
    (lib.mkIf config.aoide.kitty.enable {
      # The venue's terminal, offered to whoever needs to open one: `spawn
      # --windowed` and `resurrect` read it as `$AOIDE_TERMINAL`.
      # `-e` execs the conducted argv directly, so `{cmd}` is the bare
      # splicing form. mkDefault, so naming `aoide.terminal` in a host file
      # still wins, and so a second terminal dendrite is a conflict the
      # operator resolves rather than a silent last-one-wins.
      aoide.terminal = lib.mkDefault "kitty -e {cmd}";

      habit.home =
        { pkgs, lib, ... }:
        let
          # aoide-shell — kitty's login shell: conduct-by-default with a hard
          # safe-fallback. Order matters and every branch ends in an `exec` so a
          # broken conduct can NEVER strand the user without a shell:
            #   1. AOIDE_NO_CONDUCT set        → the plain login shell (escape hatch).
            #   2. `aoide` not on PATH         → the plain login shell (never shell-less).
            #   3. otherwise                   → `aoide conduct` the login shell, always
            #      parented to $AOIDE_SESSION_ID when already in the env (nested
            #      terminals build the graph tree). We ALWAYS wrap — a kitty spawned from
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
          programs.kitty = lib.mkMerge [
            (lib.mkForce {
              enable = true;
              package = pkgs.kitty;
              # font.name / font.size and colours are set by the stylix lane.
              settings = {
                # Conduct-by-default: every window's shell is the wrapper above.
                shell = "${aoide-shell}/bin/aoide-shell";
                scrollback_lines = 2000;
                wheel_scroll_min_lines = 1;
                confirm_os_window_close = 0;
                window_padding_width = 5;
                window_border_width = 1.5;
                # The compositor tiles every window, so kitty must not restore the
                # last-closed window's size or state: with remember_window_size on,
                # one kitty closed while maximized (a lone window on a scrolling
                # workspace) makes every later kitty open maximized.
                remember_window_size = "no";
                # Aero-glass terminal: a translucent background so the compositor's
                # blur reads through as frosted glass (the Win7-style sheen), while
                # the TEXT stays fully opaque and crisp (background_opacity fades only
                # the cell background, not the glyphs). Hyprland owns the blur pass —
                # it blurs behind any translucent surface when decoration:blur is on
                # (compositor lane, global) — so kitty's own background_blur (a
                # macOS/KDE-only path, inert under Hyprland) is turned off here and
                # the compositor does the frosting instead. The kitty window class is
                # additionally pinned in the compositor's Aero window rules.
                #
                # Brightness (the User): keep the CREAM cell colour (song base00), just
                # make the terminal read brighter — a high background_opacity (0.86)
                # so the bright cream dominates over the warm painting behind it
                # instead of the wallpaper muddying it dim; hyprglass then glosses
                # the surface on top (compositor manage_window_blur). The cream +
                # dark-ink look is unchanged; only the surface got brighter.
                #
                # This is the host's bake: what a song with no
                # `geometry.terminalOpacity` opinion shows. Nothing in cargo
                # copies it; kitty falls through to it when neither include
                # below carries a line. A song's own value arrives through those
                # includes, and open windows follow by reloading the config over
                # the control socket, which moves the opacity only because
                # dynamic_background_opacity is on.
                background_opacity = "0.86";
                dynamic_background_opacity = true;
                background_blur = 0;
                enable_audio_bell = false;
                tab_bar_style = "powerline";
                tab_powerline_style = "slanted";

                # Staged colours reach OPEN windows over kitty's control socket:
                # `rice stage` runs `kitty @ --to unix:<sock> load-config` (no
                # path: kitty re-reads this file and every include) against every
                # socket this line opens (aoide-song's live.rs finds them by the
                # `kitty-<pid>` name in the runtime dir — a directory listing, no
                # process-table walk).
                # socket-only: remote control is accepted on this socket and
                # nowhere else, never from a program writing the escape code into
                # its own pty. The socket sits in the 0700 runtime dir, so only
                # this user reaches it. kitty expands the env var and
                # `{kitty_pid}` itself: one socket per kitty instance.
                allow_remote_control = "socket-only";
                listen_on = "unix:\${XDG_RUNTIME_DIR}/kitty-{kitty_pid}";
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
            })
            # The song's terminal look, for every NEW window — two includes,
            # in this order:
            #
            #   1. `song/declared/terminal-opacity.conf` — one
            #      `background_opacity` line the lyra lane's activation seed
            #      publishes from the song's own `geometry.terminalOpacity`
            #      (CONTRACTS.md §4): the declared truth, so a host that has
            #      never staged anything still opens its terminal at the
            #      song's opacity instead of the baked default.
            #   2. `song/stage/terminal-colors.conf` — the colours `rice stage`
            #      writes, and a `background_opacity` only for a song with an
            #      opinion. SECOND, because kitty takes the LAST value for a
            #      repeated key: a live stage stays authoritative, and a stage
            #      without an opacity line falls through to the declared
            #      fragment and then the bake above.
            #
            # Both are mkAfter (after Stylix's baked base16 include) and both
            # sit outside the `mkForce` on purpose: a forced extraConfig would
            # drop Stylix's include. kitty skips a missing include with one log
            # line, so with neither file present the baked colours and opacity
            # stand. This lane reads no `aoide.livery` (root AGENTS.md rule 5)
            # — it names two paths under `aoide.root` and nothing else.
            {
              extraConfig = lib.mkAfter ''
                include ${config.aoide.root}/song/declared/terminal-opacity.conf
                include ${config.aoide.root}/song/stage/terminal-colors.conf
              '';
            }
          ];
        };
    })
  ];
}
