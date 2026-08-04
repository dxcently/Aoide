# modules/dendrites/clipboard.nix — clipboard history provider.
#
# Dendrite shape v0: guarded on aoide.clipboard.enable, carries its own
# dependencies, and keeps clipboard data in the user's runtime/cache rather
# than in the repository. The provider records text and image MIME data via
# cliphist. Display is handled by the Grimoire launcher (AoideLauncher.qml),
# which reads the clipboard backend (AoideClipboard.qml) and renders entries
# as a dedicated chapter — no separate picker surface.
{
  config,
  lib,
  pkgs,
  ...
}:
let
  clipboardCopy = pkgs.writeShellScriptBin "aoide-clipboard-copy" ''
    # Accept only the numeric cliphist identifier. Clipboard contents never
    # become shell source or an interpolated command argument.
    if [ "$#" -ne 1 ]; then
      echo "usage: aoide-clipboard-copy <cliphist-id>" >&2
      exit 2
    fi
    set -o pipefail
    case "$1" in
      ""|*[!0-9]*)
        echo "cliphist id must be numeric" >&2
        exit 2
        ;;
    esac
    CLIPHIST_DB_PATH="''${XDG_RUNTIME_DIR:?}/aoide-clipboard/db" \
      ${pkgs.cliphist}/bin/cliphist decode "$1" | ${pkgs.wl-clipboard}/bin/wl-copy
  '';

  clipboardPreview = pkgs.writeShellScriptBin "aoide-clipboard-preview" ''
    # Decode a single cliphist image entry to a private runtime preview file.
    # Only accepts a numeric cliphist id — clipboard payloads are never
    # interpolated into shell source or a command argument.
    if [ "$#" -ne 1 ]; then
      echo "usage: aoide-clipboard-preview <cliphist-id>" >&2
      exit 2
    fi
    set -o pipefail
    case "$1" in
      ""|*[!0-9]*)
        echo "cliphist id must be numeric" >&2
        exit 2
        ;;
    esac
    # Runtime preview cache — private to this user, created once per boot.
    CACHE="''${XDG_RUNTIME_DIR:?}/aoide-clipboard/previews"
    mkdir -p "$CACHE"
    chmod 700 "$CACHE" 2>/dev/null || true
    # Write to a temp file then atomically rename so QML never reads a
    # partial image.
    TMP="$CACHE/.tmp.$1.$$"
    CLIPHIST_DB_PATH="''${XDG_RUNTIME_DIR:?}/aoide-clipboard/db" \
      ${pkgs.cliphist}/bin/cliphist decode "$1" > "$TMP"
    mv -f "$TMP" "$CACHE/$1"
  '';

  # Dual watcher wrapper: one text watcher + one image watcher, both piping
  # into cliphist store.  A single `wl-paste --watch` (typeless) does not
  # reliably capture image offers when the source also offers text — cliphist
  # recommends separate type-specific watchers.  Text+image offers from the
  # same copy may create two separate cliphist entries; this is cliphist's
  # expected behaviour (it deduplicates by content hash, not source offer).
  clipboardWatch = pkgs.writeShellScriptBin "aoide-clipboard-watch" ''
    set -o pipefail
    DB="''${CLIPHIST_DB_PATH:?}"

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
  options.aoide.clipboard.enable = lib.mkEnableOption "clipboard history (cliphist + wl-clipboard, text and images)";

  config = lib.mkIf config.aoide.clipboard.enable {
    # cliphist's database lives under XDG_RUNTIME_DIR at runtime. Nothing here
    # writes clipboard data into the flake or a Nix store path.
    environment.systemPackages = [
      pkgs.cliphist
      pkgs.wl-clipboard
      clipboardCopy
      clipboardPreview
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
        ExecStart = "${clipboardWatch}/bin/aoide-clipboard-watch";
        Restart = "on-failure";
        RestartSec = "3s";
        Environment = [ "CLIPHIST_DB_PATH=%t/aoide-clipboard/db" ];

        # RuntimeDirectory is created before the sandbox is applied, so the
        # database's parent exists on the first start and is private to this
        # user. Keep the watcher constrained to the Wayland/Unix resources it
        # actually needs.
        RuntimeDirectory = "aoide-clipboard";
        RuntimeDirectoryMode = "0700";
        UMask = "0077";
        ProtectSystem = "strict";
        ReadWritePaths = [ "%t/aoide-clipboard" ];
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
