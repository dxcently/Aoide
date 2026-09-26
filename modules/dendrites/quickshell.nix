# modules/dendrites/quickshell.nix — the shell lane (the Quickshell runtime and
# its one service).
#
# Two things and nothing else: the quickshell PACKAGE, and the
# `aoide-quickshell` user service, started from the config directory a host (or
# another lane) has named. This lane renders no QML, deploys nothing, and knows
# no song: it starts the shell on whatever `aoide.quickshell.config` points at.
#
# The seam (CONTRACTS.md §0). Facts, never lanes — no module reads another
# module (root AGENTS.md house rule 5):
#   - SET here: `aoide.quickshell.enable` — "a shell surface exists on this
#     host". The lane that installs the package is the lane that knows.
#   - READ here: `aoide.quickshell.config : nullOr str` — the directory the
#     shell runs — which the `lyra` lane sets to its deployed runtime root
#     (`$AOIDE_ROOT/run/qml`) and a host may set to any config directory of its
#     own. lyra sets it, this lane reads it; neither file names the other.
#
# The two allowed shapes of quickshell WITHOUT lyra:
#   * own config — a host that brings its own directory sets
#     `aoide.quickshell.config = "/home/<u>/.config/quickshell"` (or a store
#     path) in its `nixos` block; this service runs from it.
#   * bare — `config` stays null (the default): the package is installed and no
#     service runs. No shell, no graphical-session anchor, nothing to fail.
#
# Reading discipline (house rule 5): the dress and identity scalars this lane
# genuinely uses — `aoide.livery.wallpaper` (the shell's baked cover env, so a
# wallpapered song survives a rebuild) and `aoide.root` (the runtime root the
# QML reads its stage files from). Nothing else, and no `song/` path at all:
# what this lane starts is a directory, not a song.
let
  body =
    {
      config,
      lib,
      pkgs,
      inputs,
      ...
    }:
    let
      # The Quickshell binary from the pre-declared flake input (flake.nix). The
      # upstream package lives there, not in this flake's own pkgs/ walk.
      quickshellPkg = inputs.quickshell.packages.${pkgs.stdenv.hostPlatform.system}.default;

      # The directory the shell runs. `null` is the bare shape above.
      configDir = config.aoide.quickshell.config;

      # Quickshell loads `<dir>/shell.qml` when handed a directory, so this is
      # the entry point inside whatever directory was named. Spelled out rather
      # than left to quickshell's own directory walk for one reason: the unit's
      # ExecStart and its ConditionPathExists must stay the same two strings
      # they were while this service lived in `lyra/` (see the unit's comments).
      shellEntry = "${configDir}/shell.qml";
    in
    {
      config = lib.mkIf config.aoide.quickshell.enable {

        # ── Quickshell package ──────────────────────────────────────────────────
        environment.systemPackages = [ quickshellPkg ];

        # ── Session anchor ──────────────────────────────────────────────────────
        # `aoide.sessionTarget` is the anchor rather than a fact: the core
        # `aoided` unit is `wantedBy` the target named here, and PartOf ties its
        # lifetime to it. A host with a shell names the graphical session, so the
        # daemon comes up with the compositor instead of at boot — a manually
        # started daemon left on `default.target` takes an immediate stop through
        # PartOf, and BindsTo drags the a2a/mcp doors down with it (found live on
        # sakaki). Not `mkDefault`: exactly one answer is right on this host, and
        # a second one is a conflict worth failing on.
        #
        # A graphical session exists whether or not lyra paints it, which is why
        # this rides the CONFIG fact and not a song: a host running its own
        # quickshell config is a painting host too. The bare shape names no
        # config and so anchors nothing — package only, no session claim.
        aoide.sessionTarget = lib.mkIf (configDir != null) "graphical-session.target";

        # ── Quickshell autostart via systemd user service ───────────────────────
        # The shell surface (bar/dock/wallpaper/notifications/OSD) is started by
        # a systemd user service rather than a compositor exec-once. A service is
        # the stronger session-assembly seam:
        #   - Restart=on-failure — a QML crash respawns the whole shell instead
        #     of leaving the desktop bare until the next login.
        #   - journald — `journalctl --user -u aoide-quickshell` gives real logs
        #     (an exec-once child's stderr is lost).
        #   - graphical-session.target ordering — it starts only once the
        #     compositor lane's env handoff (hyprland-session.target →
        #     graphical-session.target, WAYLAND_DISPLAY/HYPRLAND_INSTANCE_SIGNATURE
        #     imported) has fired, so Quickshell inherits a valid Wayland env.
        # ConditionPathExists guards the shell entry so the unit fails cleanly
        # (not crash-loops) if the config tree has not landed yet. That guard
        # only checks *existence*, though — a shell.qml that exists but fails to
        # load (a QML parse/load error, or an ExecStart pointed elsewhere by a
        # stray drop-in) still exits 255 and, under Restart=on-failure, would
        # respawn every RestartSec forever. StartLimit* is the backstop: after 5
        # failures inside 60s systemd stops trying and parks the unit `failed`
        # instead of thrashing the desktop (and journald) indefinitely. Five tries
        # still absorbs a genuinely transient failure (e.g. Wayland not ready yet).
        # The same condition doubles as the runtime-compose seam: a later phase
        # that materializes `run/qml/shell.qml` straight from a staged rice (no
        # rebuild) only has to write that file and start this unit —
        # ConditionPathExists is already the gate that lets it, and with a named
        # config the rebuild path already satisfies it, so nothing about this
        # unit needs to change to grow that second producer.
        #
        # Gated on the CONFIG, never on `aoide.song`: a shell that paints a
        # host's own config has no song and starts exactly the same way. The
        # song-gated half (deploy the tree, seed the stage, restart the rice)
        # is the `lyra` lane's, which is also the lane that sets this config.
        home-manager.users.${config.aoide.user} =
          { lib, ... }:
          lib.optionalAttrs (configDir != null) {
            systemd.user.services.aoide-quickshell = {
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
                ExecStart = "${quickshellPkg}/bin/quickshell -p ${shellEntry}";
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
                  # where a QGuiApplication one is not. Nothing is lost: the livery
                  # comes from song/stage/livery.json, never from Qt.
                  "QT_QPA_PLATFORMTHEME="
                  # Runtime root for every stage/state file the QML reads
                  # (livery/cover/grimoire/sessions/usage) — the QML falls back to
                  # ~/.aoide when unset, but a configured aoide.root must win.
                  "AOIDE_ROOT=${config.aoide.root}"
                ]
                # Export the song's baked wallpaper (immutable store path) so
                # AoideWallpaper always has the right cover on boot/rebuild — the live
                # stage/cover.json overrides it, but nothing re-seeded it from the
                # song before, so a rebuild lost the background. Null → no env.
                ++ lib.optionals (config.aoide.livery.wallpaper != null) [
                  "AOIDE_WALLPAPER=${config.aoide.livery.wallpaper}"
                ];
                Restart = "on-failure";
                RestartSec = 3;
              };
              Install.WantedBy = [ "graphical-session.target" ];
            };
          };
      };
    };
in
{
  inherit body;

  nixos =
    { lib, ... }:
    {
      imports = [ body ];
      config.aoide.quickshell.enable = lib.mkDefault true;
    };
}
