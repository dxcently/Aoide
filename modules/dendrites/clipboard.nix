# modules/dendrites/clipboard.nix — clipboard history provider.
#
# Dendrite shape: selection is the switch, carries its own
# dependencies, and keeps clipboard data in the user's XDG cache rather than
# in the repository — cliphist's DEFAULT db path (~/.cache/cliphist/db),
# persistent across reboots. The lane is the watcher: it records text and
# image MIME data via cliphist and installs cliphist and wl-clipboard.
# Display is the Grimoire launcher's (the rice's `launcher` slot), which reads
# the clipboard backend (pkgs/strillix-shell/qml/AoideClipboard.qml) and renders
# entries as a dedicated chapter — no separate picker surface. That backend
# copies and previews by calling the system's `cliphist` and `wl-copy` from
# PATH itself, and deletes or clears history through `strillix clipboard`, which
# runs the same `cliphist` — so it works the same on a host that runs its own
# watcher and never selects this lane.

{
  lib,
  pkgs,
  ...
}:
let
  # Dual watcher wrapper: one text watcher + one image watcher, both piping
  # into cliphist store.  A single `wl-paste --watch` (typeless) does not
  # reliably capture image offers when the source also offers text — cliphist
  # recommends separate type-specific watchers.  Text+image offers from the
  # same copy may create two separate cliphist entries; this is cliphist's
  # expected behaviour (it deduplicates by content hash, not source offer).
  clipboardWatch = pkgs.writeShellScriptBin "strillix-clipboard-watch" ''
    set -o pipefail

    cleanup() {
      kill "$TEXT_PID" "$IMAGE_PID" 2>/dev/null || true
      wait "$TEXT_PID" "$IMAGE_PID" 2>/dev/null || true
      exit 0
    }
    trap cleanup TERM INT

    ${pkgs.wl-clipboard}/bin/wl-paste --type text  --watch ${pkgs.cliphist}/bin/cliphist store &
    TEXT_PID=$!
    ${pkgs.wl-clipboard}/bin/wl-paste --type image --watch ${pkgs.cliphist}/bin/cliphist store &
    IMAGE_PID=$!

    wait "$TEXT_PID" "$IMAGE_PID"
  '';
in
{

  config = {
    # cliphist's database lives at its default XDG cache path (~/.cache/
    # cliphist/db) — persistent across reboots. Nothing here writes clipboard
    # data into the flake or a Nix store path.
    environment.systemPackages = [
      pkgs.cliphist
      pkgs.wl-clipboard
      clipboardWatch
    ];

    systemd.user.services.aoide-clipboard = {
      description = "Aoide clipboard history provider";
      wantedBy = [ "graphical-session.target" ];
      after = [ "graphical-session.target" ];
      partOf = [ "graphical-session.target" ];
      unitConfig.ConditionEnvironment = "WAYLAND_DISPLAY";

      serviceConfig = {
        # Dual type-specific watchers (text + image) wrapped in one process.
        # A typeless `wl-paste --watch` misses image offers when the source
        # also advertises text.  Text+image copies may create two cliphist
        # entries; this is expected cliphist behaviour.
        ExecStart = "${clipboardWatch}/bin/strillix-clipboard-watch";
        Restart = "on-failure";
        RestartSec = "3s";
        # The DB and previews live under ~/.cache — the watcher sandbox
        # allows writes only to those two trees. Keep it constrained to the
        # Wayland/Unix resources it actually needs.
        UMask = "0077";
        ProtectSystem = "strict";
        ReadWritePaths = [
          "%h/.cache/cliphist"
          "%h/.cache/aoide-clipboard"
        ];
        PrivateTmp = true;
        NoNewPrivileges = true;
        ProtectKernelTunables = true;
        ProtectKernelModules = true;
        ProtectControlGroups = true;
        RestrictSUIDSGID = true;
        RestrictAddressFamilies = [ "AF_UNIX" ];
        StandardOutput = "journal";
        StandardError = "journal";
      };
    };
  };
}
