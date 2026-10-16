# modules/dendrites/greeter.nix — the display-manager greeter.
#
# The login prompt, in one of two shapes chosen at build time by the DECLARED
# song (`aoide.songbook.greeter`, the fact the selection hook derives):
#
#   * The declared song ships `widgets/greeter.qml` → greetd with a Quickshell
#     greeter. greetd runs `cage` as its greeter user, and in it Quickshell on
#     a store tree this file builds: the lane's `GreeterHost.qml` as the
#     tree's `shell.qml`, `LiveryState`/`StagingEngine` beside it, the face's
#     song folder from the shipped songbook (`pkgs.lyra-songbook`), the dress
#     as `song/stage/livery.json`, and `session.json`, the command and desktop
#     name of the display manager's default session (the hyprland provider
#     names it). `AOIDE_ROOT` points at the tree,
#     so nothing reads /home and a staged song never reaches the login screen.
#     Authentication is greetd's: the host creates a session for the chosen
#     account, answers its prompts with what the face hands `respond()`, and
#     launches the user's session only once greetd reports it ready. Nothing
#     runs as the user before that — greetd's `default_session` is the
#     greeter itself, never a desktop, and there is no `initial_session`.
#   * Otherwise → `ly` on tty1, authenticating through PAM and launching the
#     same registered session. Every colour — the animation's included — is
#     an `aoide.livery` read; ly takes colours as 32-bit `0xSSRRGGBB` (a
#     styling byte ahead of the RGB: 00 plain, 01 bold) with `full_color` on
#     by default, so the palette's own hex goes in unmapped. Box title and key
#     hints stay at ly's defaults.
#
# Either way logind classes the logged-in session `user`, because PAM
# registers a real login for it.
#
# Selecting the dendrite is the switch: a host that does not select it boots
# to the tty it lands on. Reads the dress (`aoide.livery`), the song selection
# (`aoide.song`, `aoide.songbook.greeter`) and the session the display manager
# registers (`services.displayManager.sessionData`, which ly reads too) —
# root AGENTS.md house rule 5.
{
  config,
  lib,
  pkgs,
  aoideInputs,
  ...
}:
let
  # Read-side venue recolour (CONTRACTS.md §1, override tier): resolve
  # rewrites colours equal to an overridden anchor's authored value, in one
  # pass, with no option-system recursion — the option itself stays inert
  # either way.
  t = (import ../../lib/livery.nix { inherit lib; }).resolve config.aoide.livery;

  lyPlain = hex: "0x00${lib.removePrefix "#" hex}";
  lyBold = hex: "0x01${lib.removePrefix "#" hex}";

  face = config.aoide.songbook.greeter;

  quickshellPkg = aoideInputs.quickshell.packages.${pkgs.stdenv.hostPlatform.system}.default;

  # The dress as the shell reads it: the resolved livery without its override
  # tier and without null tiers (LiveryState reads a present tier as set),
  # stamped with the declared song so the staging engine resolves its face.
  dress = pkgs.writeText "aoide-greeter-livery.json" (
    builtins.toJSON (
      lib.filterAttrsRecursive (_: v: v != null) (removeAttrs t [ "override" ])
      // {
        song = config.aoide.song;
      }
    )
  );

  manifest = pkgs.writeText "aoide-greeter-manifest.json" (
    builtins.toJSON {
      ${config.aoide.song} = {
        greeter = face;
      };
    }
  );

  # The default session when one is named, else the first registered Wayland
  # session; its Exec, field codes dropped, is what the greeter launches once
  # greetd says the account is authenticated, and its file name is the
  # session's `XDG_SESSION_DESKTOP`.
  sessions = config.services.displayManager.sessionData.desktops;
  defaultSession = config.services.displayManager.defaultSession;
  pick =
    if defaultSession != null then
      ''"$dir/${defaultSession}.desktop"''
    else
      ''"$(ls "$dir"/*.desktop | head -n1)"'';

  tree = pkgs.runCommand "aoide-greeter-tree" { nativeBuildInputs = [ pkgs.jq ]; } ''
    q=$out/run/qml
    mkdir -p "$q/songs/${face.owner}" "$out/song/stage"
    cp ${pkgs.lyra-shell}/share/lyra/qml/LiveryState.qml ${pkgs.lyra-shell}/share/lyra/qml/StagingEngine.qml "$q/"
    cp ${pkgs.lyra-shell}/share/lyra/qml/GreeterHost.qml "$q/shell.qml"
    cp -r ${pkgs.lyra-songbook}/share/lyra/songbook/${face.owner}/widgets/. "$q/songs/${face.owner}/"
    cp ${manifest} "$q/songs/manifest.json"
    echo '{}' > "$q/songs/registry.json"
    cp ${dress} "$out/song/stage/livery.json"

    dir=${sessions}/share/wayland-sessions
    pick=${pick}
    exec_line=$(sed -n 's/^Exec=//p' "$pick" | head -n1)
    test -n "$exec_line" || { echo "no session Exec in $pick" >&2; exit 1; }
    jq -n --arg e "$exec_line" --arg d "$(basename "$pick" .desktop)" \
      '{command: ($e | split(" ") | map(select(length > 0 and (startswith("%") | not)))), desktop: $d}' \
      > "$out/session.json"
  '';

  # What cage runs as the greeter user. The cache goes under the greeter's
  # runtime dir: the greeter account has no home to write.
  greeterShell = pkgs.writeShellScript "aoide-greeter" ''
    export AOIDE_ROOT=${tree}
    export QT_QPA_PLATFORMTHEME=
    export QT_WAYLAND_DISABLE_WINDOWDECORATION=1
    export QT_PLUGIN_PATH=${pkgs.qt6.qtimageformats}/lib/qt-6/plugins
    export XDG_CACHE_HOME="''${XDG_RUNTIME_DIR:-/tmp}/aoide-greeter-cache"
    exec ${quickshellPkg}/bin/quickshell -p ${tree}/run/qml/shell.qml
  '';
in
{
  config = lib.mkMerge [
    (lib.mkIf (face != null) {
      services.greetd = {
        enable = true;
        settings.default_session = {
          user = "greeter";
          command = "${pkgs.cage}/bin/cage -s -m last -- ${greeterShell}";
        };
      };
    })

    (lib.mkIf (face == null) {
      services.displayManager.ly = {
        enable = true;
        settings = {
          # Behaviour lifted verbatim from dxflake's own ly dendrite
          # (modules/dendrites/displaymanager.nix on the reference rig) so
          # both rigs' greeters behave alike: the colour-mixing shader,
          # stopped after five minutes rather than painting an idle machine
          # forever, a locale clock, and a password field that clears on a
          # failed attempt.
          animation = "colormix";
          animation_timeout_sec = 300;
          clock = "%c";
          clear_password = true;

          # The dress is Aoide's, and every colour here is a palette read.
          bg = lyPlain t.palette.bg;
          fg = lyPlain t.palette.fg;
          border_fg = lyPlain t.palette.accent;
          # Bold, as ly's own default red is — an error stays emphatic.
          error_fg = lyBold t.palette.urgent;
          # The shader mixes the two chromatic anchors instead of ly's stock
          # red/blue. colormix_col3 keeps its default: its styling byte
          # (0x20) is undocumented upstream, and that is not a value to guess
          # at.
          colormix_col1 = lyPlain t.palette.accent;
          colormix_col2 = lyPlain t.palette.urgent;
        };
      };

      # Upstream gap, not a preference. nixpkgs' ly module builds
      # `systemd.services.display-manager` through the deprecated
      # `services.displayManager.generic` path, and neither that module nor
      # the shared display-manager module ever gives the unit a `wantedBy` —
      # so it is installed and never started, and the machine boots to a bare
      # tty1 with no greeter. greetd's own module carries `wantedBy = [
      # "graphical.target" ]` and aliases display-manager.service onto it, so
      # only this branch needs the link; drop it the day the ly module
      # carries it.
      systemd.services.display-manager.wantedBy = [ "graphical.target" ];
    })
  ];
}
