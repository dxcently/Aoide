# modules/facets/quickshell/default.nix — Quickshell shell-surface facet.
#
# Renders the complete Aoide shell surface using Quickshell (QML runtime).
# Surfaces owned: bar, notifications, launcher, osd, lockscreen, greeter,
# wallpaper, agentWidgets, sessionGraph.
#
# Reading discipline (CONTRACTS.md §1):
#   - Reads ONLY aoide.drachma (palette + component tiers) and aoide.surfaces.
#   - Component-tier fallback (null → palette) applied locally, never pushed
#     back into the option system.
#   - NEVER reads song/ runtime paths at build time (checks.no-song-read
#     enforces this structurally).
#
# Communication discipline (entities/Quickshell):
#   - QML reads state files from song/stage/ at runtime (hot-reload).
#   - QML issues commands via the shellbridge unix socket.
#   - QML never speaks MCP or any agent protocol.
{
  config,
  lib,
  pkgs,
  inputs,
  ...
}:
let
  cfg = config.aoide.facets.quickshell;
  t = config.aoide.drachma;

  # ── Component-tier fallback helpers ────────────────────────────────────────
  # Each is: use the component override when set, else fall back to the palette.
  # Facets apply the fallback here (CONTRACTS.md §1 rule: "Facets apply the
  # fallback, not the option system").
  barBg = if t.bar.bg != null then t.bar.bg else t.palette.bg;
  barFg = if t.bar.fg != null then t.bar.fg else t.palette.fg;
  barAccent = if t.bar.accent != null then t.bar.accent else t.palette.accent;

  notifBg = if t.notif.bg != null then t.notif.bg else t.palette.bg;
  notifFg = if t.notif.fg != null then t.notif.fg else t.palette.fg;
  notifUrgent = if t.notif.urgent != null then t.notif.urgent else t.palette.urgent;

  # ── QML root — the full skeleton config installed into ~/Aoide/qml/ ────────
  # Each surface widget is a stub that reads its colors from notes. The config
  # directory is placed in the user's Aoide tree so Quickshell picks it up at
  # session start. At runtime Quickshell hot-reloads from song/stage/drachma.json
  # via a FileView; the build only installs the structural QML, not the note
  # values themselves.
  quickshellConfig = pkgs.runCommand "aoide-quickshell-config" { } ''
    mkdir -p "$out/qml"
    cp -r ${./qml}/. "$out/qml/"
  '';

  # Path used at runtime: ~/Aoide/qml/shell.qml is Quickshell's entry point.
  # The user's fork provides ~/Aoide (aoide.user → /home/<user>/Aoide), so we
  # activate Quickshell pointing at that tree.
  shellQmlEntry = "/home/${config.aoide.user}/Aoide/qml/shell.qml";

  # The Quickshell binary from the pre-declared flake input (flake.nix).
  quickshellPkg = inputs.quickshell.packages.${pkgs.stdenv.hostPlatform.system}.default;
in
{
  # ── Option: aoide.facets.quickshell.enable ─────────────────────────────────
  options.aoide.facets.quickshell = {
    enable = lib.mkEnableOption "Quickshell shell-surface facet";
  };

  # ── Config: only wired when the facet is enabled ───────────────────────────
  config = lib.mkIf cfg.enable {

    # ── Surface-ownership registry ──────────────────────────────────────────
    # Declare every Quickshell-owned surface. Stylix reads this registry and
    # stands down for these surfaces (concepts/Notes).
    aoide.surfaces = {
      bar.owner = "quickshell";
      notifications.owner = "quickshell";
      launcher.owner = "quickshell";
      osd.owner = "quickshell";
      lockscreen.owner = "quickshell";
      greeter.owner = "quickshell";
      wallpaper.owner = "quickshell";
      agentWidgets.owner = "quickshell";
      sessionGraph.owner = "quickshell";
    };

    # ── Quickshell package ──────────────────────────────────────────────────
    # The upstream Quickshell package lives in the pre-declared flake input
    # (flake.nix wires inputs.quickshell for exactly this).
    environment.systemPackages = [
      quickshellPkg
    ];

    # ── Install QML config tree into the user's Aoide fork ──────────────────
    # The config tree (bar/notif/launcher/OSD/lockscreen/greeter/wallpaper)
    # lives at /home/<user>/Aoide/qml/ so it's under git control in the fork.
    # We write it via home-manager activation rather than system packages so
    # the path lands in the user's home tree.
    home-manager.users.${config.aoide.user} = {
      home.file."Aoide/qml" = {
        source = "${quickshellConfig}/qml";
        recursive = true;
      };

      # ── Quickshell autostart via systemd user service ─────────────────────
      # The shell surface (bar/dock/wallpaper/notifications/OSD) is started by
      # a systemd user service rather than a Hyprland exec-once. A service is
      # the stronger session-assembly seam:
      #   - Restart=on-failure — a QML crash respawns the whole shell instead
      #     of leaving the desktop bare until the next login.
      #   - journald — `journalctl --user -u aoide-quickshell` gives real logs
      #     (an exec-once child's stderr is lost).
      #   - graphical-session.target ordering — it starts only once the
      #     compositor facet's env handoff (hyprland-session.target →
      #     graphical-session.target, WAYLAND_DISPLAY/HYPRLAND_INSTANCE_SIGNATURE
      #     imported) has fired, so Quickshell inherits a valid Wayland env.
      # ConditionPathExists guards the shell entry so the unit fails cleanly
      # (not crash-loops) if the QML tree hasn't landed in the fork yet. That
      # guard only checks *existence*, though — a shell.qml that exists but
      # fails to load (a QML parse/load error, or an ExecStart pointed elsewhere
      # by a stray drop-in) still exits 255 and, under Restart=on-failure, would
      # respawn every RestartSec forever. StartLimit* is the backstop: after 5
      # failures inside 60s systemd stops trying and parks the unit `failed`
      # instead of thrashing the desktop (and journald) indefinitely. Five tries
      # still absorbs a genuinely transient failure (e.g. Wayland not ready yet).
      systemd.user.services.aoide-quickshell = {
        Unit = {
          Description = "Aoide Quickshell — shell surface (bar/dock/wallpaper/notifications)";
          PartOf = [ "graphical-session.target" ];
          After = [ "graphical-session.target" ];
          ConditionEnvironment = "WAYLAND_DISPLAY";
          ConditionPathExists = shellQmlEntry;
          StartLimitIntervalSec = 60;
          StartLimitBurst = 5;
        };
        Service = {
          # `-p <path>` loads a config by PATH; `-c <name>` (used previously)
          # treats the argument as a config NAME and fails on a path in
          # quickshell 0.3.0.
          ExecStart = "${quickshellPkg}/bin/quickshell -p ${shellQmlEntry}";
          # Qt6's qtbase ships only jpeg/png/gif/ico imageformats plugins (plus
          # qtsvg); webp/tiff/etc. live in a SEPARATE qtimageformats plugin the
          # quickshell wrapper does not carry. A song cover may be any of those
          # formats (sonata's is .webp), and AoideWallpaper renders it through a
          # Qt Image — with no webp plugin the Image can't decode and the layer
          # falls back to the solid palette colour (the "wallpaper didn't run
          # after rebuild" symptom). The wrapper sets QT_PLUGIN_PATH via
          # makeWrapper --prefix, which PRESERVES an inherited value, so this
          # plugin dir is scanned alongside the wrapper's own. quickshell follows
          # this flake's nixpkgs, so this qtimageformats is the exact Qt ABI.
          Environment = [
            "QT_PLUGIN_PATH=${pkgs.qt6.qtimageformats}/lib/qt-6/plugins"
          ]
          # Export the song's baked wallpaper (immutable store path) so
          # AoideWallpaper always has the right cover on boot/rebuild — the live
          # stage/cover.json overrides it, but nothing re-seeded it from the
          # song before, so a rebuild lost the background. Null → no env.
          ++ lib.optionals (config.aoide.drachma.wallpaper != null) [
            "AOIDE_WALLPAPER=${config.aoide.drachma.wallpaper}"
          ];
          Restart = "on-failure";
          RestartSec = 3;
        };
        Install.WantedBy = [ "graphical-session.target" ];
      };
    };
  };
}
