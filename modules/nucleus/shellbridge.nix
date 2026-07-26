# modules/nucleus/shellbridge.nix — shell integration bridge service.
#
# shellbridge is the bidirectional bridge between the daemon / agents and the
# live desktop (entities/shellbridge.md, concepts/Desktop-Architecture.md):
#
#   OUT: atomic JSON state files → song/stage/*.json (read by Quickshell).
#   IN:  unix socket commands    ← agents, CLI, Quickshell QML widgets.
#   IPC: Hyprland IPC consumed here, never in QML.
#
# Design constraints (hard architectural rules, not suggestions):
#   - No MCP in QML, ever. All agent↔shell traffic routes through this socket.
#   - QML is a display layer: it reads song/stage/*.json and issues socket
#     commands; it never speaks an agent protocol.
#   - Writes to song/stage/ are atomic (write-temp-then-rename) so a
#     Quickshell hot-reload never reads a torn file (CONTRACTS.md §4).
#
# Session-jump flow (Terminal Commander, concepts/Terminal-Commander.md):
#   widget click → shellbridge unix socket → hyprctl dispatch focuswindow address:…
#
# Session registration: when the aoide spawn wrapper starts a claude-CLI
# session it registers { sessionId, windowAddress } with shellbridge via the
# socket. The roster is written atomically to song/stage/sessions.json;
# the Quickshell bar widget reads it for the Terminal Commander roster.
#
# Hook states (Notification / Stop / Pre-PostToolUse) are posted through
# shellbridge, making the bar's connection-state widget a live reflection of
# the agent's execution phase.
#
# The socket and stage paths below are the stable integration seams; adapters
# and Quickshell widgets must use these exact paths (CONTRACTS.md §4).
{ config, lib, pkgs, ... }:

lib.mkIf config.aoide.enable {

  # ── Runtime directories ──────────────────────────────────────────────────
  # song/stage/ is shared with aoided.nix's tmpfiles rules; systemd-tmpfiles
  # deduplicates identical rules so declaring it here as well is safe.
  systemd.user.tmpfiles.rules = [
    "d %h/Aoide/song/stage 0755 - - -"
  ];

  # ── shellbridge systemd user service ─────────────────────────────────────
  systemd.user.services.shellbridge = {
    description = "Aoide shellbridge — desktop ↔ agent bridge (socket in, stage-files out)";

    wantedBy = [ "graphical-session.target" ];
    after    = [ "graphical-session.target" "aoided.service" ];
    partOf   = [ "graphical-session.target" ];

    serviceConfig = {
      # shellbridge is a sub-command of the aoide binary (skeleton). Agent B
      # supplies the real implementation; referencing pkgs.aoide keeps eval
      # clean independent of whether the binary is realised.
      ExecStart = "${pkgs.aoide}/bin/aoide shellbridge --run";

      Restart    = "on-failure";
      RestartSec = "3s";

      Environment = [
        # Stable socket path — adapters and QML widgets bind to this.
        # Convention: XDG_RUNTIME_DIR is available inside user services.
        "AOIDE_BRIDGE_SOCKET=%t/aoide/shellbridge.sock"
        # Stage directory for atomic JSON state files (CONTRACTS.md §4).
        "AOIDE_STAGE_DIR=%h/Aoide/song/stage"
        # Hyprland socket (standard Hyprland env; shellbridge reads it directly).
        # HYPRLAND_INSTANCE_SIGNATURE is set by the compositor at session start.
        "AOIDE_USER=${config.aoide.user}"
      ];

      # Create the socket directory under XDG_RUNTIME_DIR.
      RuntimeDirectory = "aoide";
      RuntimeDirectoryMode = "0700";

      NoNewPrivileges = true;
      StandardOutput  = "journal";
      StandardError   = "journal";
    };
  };

  # ── Stage-file paths exposed as options for downstream modules ────────────
  # These are the stable v0 stage paths (CONTRACTS.md §4). Facets and the
  # Quickshell widget must read exactly these paths; never compute them
  # independently. Note: these are RUNTIME paths — they are NEVER imported
  # by any nix module (checks.no-song-read enforces this).
  #
  # Documented here as comments (not as options) because they are live-side
  # constants, not build-time configuration:
  #
  #   song/stage/tokens.json    — resolved token colours (written by pkgs/tokens)
  #   song/stage/sessions.json  — agent session roster (written by shellbridge)
  #   song/stage/hooks.json     — live Claude Code hook states (written by shellbridge)
  #
  # sessions.json schema (v0):
  # {
  #   "schemaVersion": "0",
  #   "sessions": [
  #     {
  #       "sessionId":     "string — claude-CLI session identifier",
  #       "agent":         "string — 'claude' | 'melete-run' | …",
  #       "windowAddress": "string — Hyprland window address for focuswindow dispatch",
  #       "cwd":           "string — working directory / repo path",
  #       "state":         "string — 'running' | 'awaiting-input' | 'idle' | 'done'",
  #       "startedAt":     "ISO-8601 timestamp"
  #     }
  #   ]
  # }
  #
  # hooks.json schema (v0):
  # {
  #   "schemaVersion": "0",
  #   "hooks": [
  #     {
  #       "sessionId": "string",
  #       "phase":     "string — 'Notification' | 'Stop' | 'PreToolUse' | 'PostToolUse'",
  #       "updatedAt": "ISO-8601 timestamp"
  #     }
  #   ]
  # }
}
