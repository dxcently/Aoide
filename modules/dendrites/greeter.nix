# modules/dendrites/greeter.nix — the `greeter` slot's strillix provider: the
# display-manager greeter.
#
# It is the one strillix provider that does not bring the strillix runtime. Every
# other provider imports the runtime pair; this one runs in greetd before any
# user session, as the greeter user under cage in its own process, on a store
# tree of its own, so it cannot share the logged-in user's runtime import
# (`strillix/README.md`).
#
# The login prompt, in one of two shapes chosen at build time by the DECLARED
# rice (`strillix.rices.greeter`, the fact the selection hook derives):
#
#   * The declared rice ships `widgets/greeter.qml` → greetd with a Quickshell
#     greeter. greetd runs `cage` as its greeter user, and in it Quickshell on
#     a store tree this file builds: the lane's `GreeterHost.qml` as the
#     tree's `shell.qml`, `ThemeState`/`StagingEngine` beside it, the face's
#     rice folder from the shipped rices (`pkgs.strillix-rices`) with a
#     `qmldir` naming its uppercase helpers as types (the face loads by a
#     `qs:` URL, where a sibling file is no type otherwise — strillix's tree
#     writes the same), that
#     rice's `icons/` (when it has one) at `rices/<owner>/icons` —
#     the `$AOIDE_ROOT`-relative path a face loads its icons from in the
#     session too — its `wallpaper/` folder at `rices/<owner>/wallpaper`,
#     with `stage/wallpaper.json` naming the logon picture
#     (`<owner>-logon.<ext>`) as the face's background when it ships one,
#     the dress as `stage/theme.json`, and
#     `session.json`: the display manager's
#     session wrapper (`services.displayManager.sessionData.wrapper`, the
#     one ly runs — it sources the profile and evaluates the Exec line with
#     the shell's own quoting) followed by the default session's Exec line
#     whole, field codes dropped, plus the session's desktop name. The
#     default session must be named (`services.displayManager.defaultSession`;
#     the hyprland provider names it) — evaluation fails otherwise, never a
#     guess. cage takes its keyboard layout from the host's
#     `services.xserver.xkb`, so a password types as it will in the session.
#     cage shows the greeter on the last-connected output, or, with
#     `strillix.greeter.output` set, runs `-m extend` with every other output
#     turned off by wlr-randr before Quickshell starts. A named output that is
#     missing or disabled turns nothing off, so the greeter spans every
#     connected output.
#     `AOIDE_ROOT` and `STRILLIX_ROOT` point at the tree,
#     so nothing reads /home and a staged rice never reaches the login screen.
#     Authentication is greetd's: the host creates a session for the chosen
#     account, answers its prompts with what the face hands `respond()`, and
#     launches the user's session only once greetd reports it ready. Nothing
#     runs as the user before that — greetd's `default_session` is the
#     greeter itself, never a desktop, and there is no `initial_session`.
#   * Otherwise → `ly` on tty1, authenticating through PAM and launching the
#     same registered session. Every colour — the animation's included — is
#     an `strillix.theme` read; ly takes colours as 32-bit `0xSSRRGGBB` (a
#     styling byte ahead of the RGB: 00 plain, 01 bold) with `full_color` on
#     by default, so the palette's own hex goes in unmapped. Box title and key
#     hints stay at ly's defaults.
#
# Either way logind classes the logged-in session `user`, because PAM
# registers a real login for it.
#
# Filling the slot is the switch: a host that does not select it boots to the
# tty it lands on. Reads the dress (`strillix.theme`), the rice selection
# (`strillix.rice`, `strillix.rices.greeter`), the session the display manager
# registers (`services.displayManager.sessionData`, which ly reads too), the
# host's keyboard (`services.xserver.xkb`) and its own output
# (`strillix.greeter.output`) — root AGENTS.md house rule 5.
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
  t = (import ../../lib/theme.nix { inherit lib; }).resolve config.strillix.theme;

  lyPlain = hex: "0x00${lib.removePrefix "#" hex}";
  lyBold = hex: "0x01${lib.removePrefix "#" hex}";

  face = config.strillix.rices.greeter;
  output = config.strillix.greeter.output;
  cageMode = if output == null then "last" else "extend";

  quickshellPkg = aoideInputs.quickshell.packages.${pkgs.stdenv.hostPlatform.system}.default;

  # The dress as the shell reads it: the resolved theme without its override
  # tier and without null tiers (ThemeState reads a present tier as set),
  # stamped with the declared rice so the staging engine resolves its face.
  dress = pkgs.writeText "aoide-greeter-theme.json" (
    builtins.toJSON (
      lib.filterAttrsRecursive (_: v: v != null) (removeAttrs t [ "override" ])
      // {
        rice = config.strillix.rice;
      }
    )
  );

  manifest = pkgs.writeText "aoide-greeter-manifest.json" (
    builtins.toJSON {
      ${config.strillix.rice} = {
        greeter = face;
      };
    }
  );

  # The default session's Exec, field codes dropped and otherwise whole, run
  # by the display manager's session wrapper; its file name is the session's
  # `XDG_SESSION_DESKTOP`.
  sessions = config.services.displayManager.sessionData.desktops;
  defaultSession =
    if config.services.displayManager.defaultSession != null then
      config.services.displayManager.defaultSession
    else
      throw "greeter: the Quickshell greeter launches services.displayManager.defaultSession, and none is set; name the session (the compositor provider sets it).";
  wrapper = config.services.displayManager.sessionData.wrapper;
  xkb = config.services.xserver.xkb;

  tree = pkgs.runCommand "aoide-greeter-tree" { nativeBuildInputs = [ pkgs.jq ]; } ''
    q=$out/run/qml
    mkdir -p "$q/rices/${face.owner}" "$out/stage"
    cp ${pkgs.strillix-shell}/share/strillix/qml/ThemeState.qml ${pkgs.strillix-shell}/share/strillix/qml/StagingEngine.qml "$q/"
    cp ${pkgs.strillix-shell}/share/strillix/qml/GreeterHost.qml "$q/shell.qml"
    rice=${pkgs.strillix-rices}/share/strillix/rices/${face.owner}
    cp -r "$rice/widgets/." "$q/rices/${face.owner}/"
    for h in "$rice/widgets/"[A-Z]*.qml; do
      [ -e "$h" ] || continue
      t=$(basename "$h" .qml)
      printf '%s 1.0 %s.qml\n' "$t" "$t" >> "$q/rices/${face.owner}/qmldir"
    done
    if [ -d "$rice/icons" ]; then
      mkdir -p "$out/rices/${face.owner}"
      cp -r "$rice/icons" "$out/rices/${face.owner}/icons"
    fi
    # The face's wallpaper folder, whole, at the same `rices/<owner>/` path
    # the session reads it from. Its logon picture (`<owner>-logon.<ext>`), when
    # it ships one, is the background the logon face draws: `stage/wallpaper.json`
    # names it in the one shape every surface reads a wallpaper by, stamped with the
    # declared rice the way the seed stamps its default.
    if [ -d "$rice/wallpaper" ]; then
      mkdir -p "$out/rices/${face.owner}"
      cp -r "$rice/wallpaper" "$out/rices/${face.owner}/wallpaper"
      for ext in webp png jpg jpeg; do
        logon="$out/rices/${face.owner}/wallpaper/${face.owner}-logon.$ext"
        if [ -f "$logon" ]; then
          jq -n --arg p "$logon" --arg s ${lib.escapeShellArg config.strillix.rice} '{path: $p, rice: $s}' \
            > "$out/stage/wallpaper.json"
          break
        fi
      done
    fi
    cp ${manifest} "$q/rices/manifest.json"
    echo '{}' > "$q/rices/registry.json"
    cp ${dress} "$out/stage/theme.json"

    pick=${sessions}/share/wayland-sessions/${defaultSession}.desktop
    test -f "$pick" || { echo "the default session ${defaultSession} is no registered Wayland session" >&2; exit 1; }
    exec_line=$(sed -n 's/^Exec=//p' "$pick" | head -n1 | sed -E ':a; s/(^| )%[a-zA-Z]( |$)/\1/; ta; s/%%/%/g')
    test -n "$exec_line" || { echo "no session Exec in $pick" >&2; exit 1; }
    jq -n --arg w ${wrapper} --arg e "$exec_line" --arg d ${defaultSession} \
      '{command: [$w, $e], desktop: $d}' > "$out/session.json"
  '';

  # Turns every output but `output` off before quickshell starts, so the
  # greeter is on that one alone. Only enabled outputs are listed, so a named
  # output that is missing or disabled takes the else branch and turns nothing
  # off; cage runs as `-m extend` then, so the greeter spans every connected
  # output. Empty when no output is named, so the script stays as it was.
  outputOnly = lib.optionalString (output != null) ''
    json=$(${pkgs.wlr-randr}/bin/wlr-randr --json) || { echo "aoide-greeter: wlr-randr failed; every output stays on" >&2; json=; }
    names=$(printf '%s\n' "$json" | ${pkgs.jq}/bin/jq -r '.[] | select(.enabled) | .name') || { echo "aoide-greeter: could not read the outputs; every output stays on" >&2; names=; }
    if printf '%s\n' "$names" | ${pkgs.gnugrep}/bin/grep -qxF ${lib.escapeShellArg output}; then
      printf '%s\n' "$names" | while IFS= read -r name; do
        [ -n "$name" ] && [ "$name" != ${lib.escapeShellArg output} ] || continue
        ${pkgs.wlr-randr}/bin/wlr-randr --output "$name" --off ||
          echo "aoide-greeter: could not turn $name off; it stays on" >&2
      done
    else
      echo "aoide-greeter: output" ${lib.escapeShellArg output} "is not connected or enabled; cage runs -m extend, so the greeter spans every connected output" >&2
    fi
  '';

  # What cage runs as the greeter user. The cache goes under the greeter's
  # runtime dir: the greeter account has no home to write.
  greeterShell = pkgs.writeShellScript "aoide-greeter" ''
    export AOIDE_ROOT=${tree}
    export STRILLIX_ROOT=${tree}
    export XKB_DEFAULT_LAYOUT=${lib.escapeShellArg xkb.layout}
    export XKB_DEFAULT_VARIANT=${lib.escapeShellArg xkb.variant}
    export XKB_DEFAULT_OPTIONS=${lib.escapeShellArg xkb.options}
    export XKB_DEFAULT_MODEL=${lib.escapeShellArg xkb.model}
    export QT_QPA_PLATFORMTHEME=
    export QT_WAYLAND_DISABLE_WINDOWDECORATION=1
    export QT_PLUGIN_PATH=${pkgs.qt6.qtimageformats}/lib/qt-6/plugins
    export XDG_CACHE_HOME="''${XDG_RUNTIME_DIR:-/tmp}/aoide-greeter-cache"
    ${outputOnly}exec ${quickshellPkg}/bin/quickshell -p ${tree}/run/qml/shell.qml
  '';
in
{
  options.strillix.greeter.output = lib.mkOption {
    type = lib.types.nullOr lib.types.nonEmptyStr;
    default = null;
    example = "DP-1";
    description = ''
      The output connector (as the compositor names it, e.g. `"DP-1"`) the
      Quickshell greeter shows on; every other output is turned off while the
      greeter runs. A named output that is missing or disabled turns nothing
      off: cage runs as `-m extend`, so the greeter spans every connected
      output. `null` leaves cage on the last-connected output. Only the
      Quickshell greeter reads it; ly ignores it.
    '';
  };

  config = lib.mkMerge [
    {
      strillix.components.greeter.provider = "strillix";
      strillix.surfaces.greeter.owner = "quickshell";
    }

    (lib.mkIf (face != null) {
      services.greetd = {
        enable = true;
        settings.default_session = {
          user = "greeter";
          command = "${pkgs.cage}/bin/cage -s -m ${cageMode} -- ${greeterShell}";
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
