# modules/dendrites/quickshell/home.nix — the shell lane's home half: the
# `aoide-quickshell` user service on the config directory `strillix.quickshell.config`
# names, and the quickshell package where the system half is not (`quickshell.nixos`).
# Imported through `habit.home` by the catalogue entry (default.nix) and by the
# strillix runtime's home half (runtime-home.nix), so one file serves both.
{
  config,
  lib,
  pkgs,
  options,
  aoideInputs,
  ...
}:
let
  quickshellPkg = aoideInputs.quickshell.packages.${pkgs.stdenv.hostPlatform.system}.default;
  configDir = config.strillix.quickshell.config;
  shellEntry = "${configDir}/shell.qml";
in
{
  config =
    if !(options ? strillix) then
      throw ''
        strillix: this home has no strillix options, so the nucleus module is missing. Build the home
        through the constructor with it in extraModules:
          mkHome { ...; extraModules = [ aoide.homeModules.nucleus ]; }
      ''
    else
      {
        strillix.quickshell.enable = true;
        # The package is the system half's on NixOS (system.nix); a standalone home installs it.
        home.packages = lib.mkIf (!config.strillix.quickshell.nixos) [ quickshellPkg ];

        # ── Quickshell autostart via systemd user service ───────────────────────
        # The shell surface (bar/dock/wallpaper/notifications/OSD) is started by
        # a systemd user service rather than a compositor exec-once. A service is
        # the stronger session-assembly seam:
        #   - Restart=always — a QML crash or a clean exit respawns the whole
        #     shell instead of leaving the desktop bare until the next login.
        #     `systemctl --user stop` is never fought, so a deliberate stop holds.
        #   - journald — `journalctl --user -u aoide-quickshell` gives real logs
        #     (an exec-once child's stderr is lost), at quickshell's INFO level,
        #     and ExecStopPost closes every run with an `aoide-quickshell ended:
        #     result= code= status=` line, so a stop names its cause.
        #   - graphical-session.target ordering — it starts only once the
        #     compositor lane's env handoff (hyprland-session.target →
        #     graphical-session.target, WAYLAND_DISPLAY/HYPRLAND_INSTANCE_SIGNATURE
        #     imported) has fired, so Quickshell inherits a valid Wayland env.
        # ConditionPathExists guards the shell entry so the unit fails cleanly
        # (not crash-loops) if the config tree has not landed yet. That guard
        # only checks *existence*, though — a shell.qml that exists but fails to
        # load (a QML parse/load error, or an ExecStart pointed elsewhere by a
        # stray drop-in) still exits 255 and, under Restart=always, would
        # respawn every RestartSec forever. StartLimit* is the backstop: after 5
        # starts inside 60s systemd stops trying and parks the unit `failed`
        # instead of thrashing the desktop (and journald) indefinitely. Five tries
        # still absorbs a genuinely transient failure (e.g. Wayland not ready yet).
        # The same condition doubles as the runtime-compose seam: a later phase
        # that materializes `run/qml/shell.qml` straight from a staged rice (no
        # rebuild) only has to write that file and start this unit —
        # ConditionPathExists is already the gate that lets it, and with a named
        # config the rebuild path already satisfies it, so nothing about this
        # unit needs to change to grow that second producer.
        #
        # Gated on the CONFIG, never on `strillix.rice`: a shell that paints a
        # host's own config has no rice and starts exactly the same way. The
        # rice-gated half (deploy the tree, seed the stage, restart the rice)
        # is the `strillix` lane's, which is also the lane that sets this config.
        systemd.user.services = lib.mkIf (configDir != null) {
          aoide-quickshell = {
            Unit = {
              Description = "Aoide Quickshell — shell surface (bar/dock/wallpaper/notifications)";
              PartOf = [ "graphical-session.target" ];
              After = [ "graphical-session.target" ];
              ConditionEnvironment = "WAYLAND_DISPLAY";
              ConditionPathExists = shellEntry;
              StartLimitIntervalSec = 60;
              StartLimitBurst = 5;
            };
            Service = {
              # `-p <path>` loads a config by PATH; `-c <name>` (used previously)
              # treats the argument as a config NAME and fails on a path in
              # quickshell 0.3.0.
              ExecStart = "${quickshellPkg}/bin/quickshell -v -p ${shellEntry}";
              # The shell, not systemd, expands these from ExecStopPost's
              # environment, so the words stay inside one quoted argument.
              ExecStopPost = "${pkgs.bash}/bin/sh -c 'echo \"aoide-quickshell ended: result=$SERVICE_RESULT code=$EXIT_CODE status=$EXIT_STATUS\"'";
              # Qt6's qtbase ships only jpeg/png/gif/ico imageformats plugins (plus
              # qtsvg); webp/tiff/etc. live in a SEPARATE qtimageformats plugin the
              # quickshell wrapper does not carry. A rice wallpaper may be any of those
              # formats (sonata's is .webp), and AoideWallpaper renders it through a
              # Qt Image — with no webp plugin the Image can't decode and the layer
              # falls back to the solid palette colour (the "wallpaper didn't run
              # after rebuild" symptom). The wrapper sets QT_PLUGIN_PATH via
              # makeWrapper --prefix, which PRESERVES an inherited value, so this
              # plugin dir is scanned alongside the wrapper's own. quickshell follows
              # this flake's nixpkgs, so this qtimageformats is the exact Qt ABI.
              Environment = [
                "QT_PLUGIN_PATH=${pkgs.qt6.qtimageformats}/lib/qt-6/plugins"
                # No Qt platform theme for this unit. quickshell follows this
                # flake's nixpkgs while the host's Qt style plugins come from the
                # system's, and a style plugin built against a different qtbase
                # PATCH release cannot load — Qt tags its private symbols per
                # patch (`Qt_6_PRIVATE_API`) precisely to forbid that mix. qt6ct
                # does not fall back when the load fails: QStyleFactory returns
                # null and the QApplication constructor deadlocks before the QML
                # engine starts, so the shell registers zero layer surfaces and
                # logs nothing past its startup banner (chiyo + osaka, blank
                # desktops, 2026-09-03). shell.qml's `UseQApplication` pragma is
                # what instantiates a QStyle at all, so this unit is exposed
                # where a QGuiApplication one is not. Nothing is lost: the theme
                # comes from $STRILLIX_ROOT/stage/theme.json, never from Qt.
                "QT_QPA_PLATFORMTHEME="
                # Runtime root for every stage/state file the QML reads
                # (theme/wallpaper/grimoire/sessions/usage) — the QML falls back to
                # ~/.aoide when unset, but a configured aoide.root must win.
                "AOIDE_ROOT=${config.aoide.root}"
                "STRILLIX_ROOT=${config.strillix.root}"
              ]
              # Export the rice's baked wallpaper (immutable store path) so
              # AoideWallpaper always has the right wallpaper on boot/rebuild — the live
              # stage/wallpaper.json overrides it, but nothing re-seeded it from the
              # rice before, so a rebuild lost the background. Null → no env.
              ++ lib.optionals (config.strillix.theme.wallpaper != null) [
                "AOIDE_WALLPAPER=${config.strillix.theme.wallpaper}"
              ];
              Restart = "always";
              RestartSec = 3;
            };
            Install.WantedBy = [ "graphical-session.target" ];
          };
        };

      };
}
