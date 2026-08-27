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
{
  config,
  lib,
  pkgs,
  inputs,
  ...
}:

let
  # The same Quickshell package the facet installs (its default.nix) — the
  # daemon's rice-toggle path re-execs `aoide rice …`, whose widget-sync half
  # shells out to `quickshell ipc call shell reload`; the client binary must
  # be the build the live shell actually runs.
  quickshellPkg = inputs.quickshell.packages.${pkgs.stdenv.hostPlatform.system}.default;
in

# Gated on the quickshell facet, not bare aoide.enable: everything in this
# file is graphical-session machinery (the bridge serves the painted shell;
# the reap units are wantedBy/partOf graphical-session.target and never fire
# headless). On a headless aoide box (aoide.enable + the doors only) these
# units would sit inert while their PATH entries (quickshell, hyprland,
# hyprlock) drag the whole Qt/Wayland stack into the closure — found live on
# sakaki, whose only aoide duty is the A2A door.
#
# Also gated on aoide.lyra.enable (P-A8 of the binary-split workstream):
# ExecStart below execs lyra out of pkgs.aoide.rice, a SEPARATE, droppable
# output (P-A8's multi-output split). aoide.lyra.enable defaults to the
# quickshell facet's own enablement, so this changes nothing for an
# unconfigured host — but without this second gate, a host that left the
# facet on and explicitly flipped aoide.lyra.enable off would still start
# this unit and exec a binary no longer in its closure. Least-surprise pick:
# gate the unit rather than assert the combination is an error, since
# "quickshell but no lyra" is a legitimate (if unusual) configuration this
# option exists to allow.
lib.mkIf (config.aoide.enable && config.aoide.facets.quickshell.enable && config.aoide.lyra.enable) {

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
    after = [
      "graphical-session.target"
      "aoided.service"
    ];
    partOf = [ "graphical-session.target" ];

    # hyprctl MUST be on the service PATH: the socket handler's session-jump
    # (focus_window) and the window→session event listener both shell out to
    # `hyprctl`. A systemd user unit's default PATH is minimal (coreutils &c.)
    # and does NOT include the compositor, so without this every widget click
    # failed with `hyprctl unavailable: No such file or directory` and the
    # listener could not read `hyprctl clients` — the click never jumped.
    # (This is a unit-level option, NOT a serviceConfig key.)
    #
    # Everything else the daemon can reach rides the same PATH — each entry
    # is a bare-name spawn inside an `aoide` subcommand the socket handler
    # re-execs, each verified absent from the default unit PATH:
    #   curl       — UsageGadget's ❋ refresh: RefreshUsage re-execs
    #                `aoide usage`, whose live claude.ai fetch spawns `curl`;
    #                without it every daemon-routed refresh degraded to
    #                {error: "curl failed"} while a shell-run succeeded.
    #   hyprlock   — the powermenu's `lock` (PowerAction::Lock).
    #   libnotify  — `notify-send`: the rice-mode toggle's success toast.
    #   procps     — `kill`: the stray-process sweep `rice mode stage` opens
    #                with (reap_stray_processes); systemctl is already covered
    #                by the default PATH's systemd package.
    #   quickshell — `quickshell ipc call shell reload` when a daemon-routed
    #                `rice mode stage` syncs changed widget bodies.
    path = [
      pkgs.curl
      pkgs.hyprland
      pkgs.hyprlock
      pkgs.libnotify
      pkgs.procps
      quickshellPkg
    ];

    serviceConfig = {
      # shellbridge is a sub-command of the aoide binary. `--run` seeds the
      # stage files, then binds the unix socket and serves commands on a
      # blocking accept loop — a long-running foreground process, so the default
      # Type=simple is correct (declared explicitly here) and keeps the unit
      # active on the loop rather than treating an immediate return as done.
      Type = "simple";
      # `shellbridge` left core's registry at P-A5 of the binary-split
      # workstream — it now lives only in `lyra` (crates/lyra/src/registry.rs;
      # core's registration lines were dropped, per the doc comment at
      # crates/cli/src/commands/mod.rs::all()). Both binaries ship from the
      # same `pkgs.aoide` derivation (P-A7), but as of P-A8 `lyra` lives in
      # that derivation's separate `rice` output (`pkgs.aoide.rice`) —
      # sibling resolution breaks across store paths (protocol::bin's
      # resolver walks current_exe's own directory), so this must name the
      # rice output explicitly rather than lean on that inference.
      ExecStart = "${pkgs.aoide.rice}/bin/lyra shellbridge --run";

      Restart = "on-failure";
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
        # Nix-declared baseline song — same env-baked-into-the-service
        # precedent as quickshell's AOIDE_WALLPAPER (modules/facets/quickshell/
        # default.nix). Read by dispatch_rice_mode_toggle's declarative-
        # direction re-exec (shellbridge.rs) so the bar's rice-mode toggle
        # re-pins to the shipped baseline instead of whatever song happens to
        # be staged.
        "AOIDE_DEFAULT_SONG=${config.aoide.song}"
        # The process running this unit IS lyra now, so shellbridge's core_bin()
        # re-exec sites (protocol::bin's sibling resolver — the usage/recheck
        # calls, shellbridge.rs) need this set explicitly (P-A7 of the
        # binary-split workstream). As of P-A8 that is no longer optional
        # belt-and-suspenders: `lyra` runs out of the `rice` output while
        # `aoide` stays in `out` — two different store paths — so the
        # sibling-directory inference would not find `aoide` next to `lyra`
        # even if left to it.
        "AOIDE_CORE_BIN=${pkgs.aoide}/bin/aoide"
      ];

      # Create the socket directory under XDG_RUNTIME_DIR.
      RuntimeDirectory = "aoide";
      RuntimeDirectoryMode = "0700";

      NoNewPrivileges = true;
      StandardOutput = "journal";
      StandardError = "journal";
    };
  };

  # ── Liveness reaper: timer + oneshot service ─────────────────────────────
  # A terminal killed with SUPER+Q / SIGKILL is torn down uncatchably, so the
  # `conduct`/`wrap` process can never run its own `graph session end` — the
  # session record is stranded `running` in the roster forever (22 dead
  # `conduct-*` shells piled up in ~8 minutes of use). No QML surface can fix
  # this: widgets/conductor cannot spawn `hyprctl` (no process-spawning in QML).
  #
  # So the sweep lives HERE, next to the stage/graph infra it repairs: a cheap
  # periodic `aoide graph reap` that gathers live `hyprctl clients -j` window
  # addresses (falling back to pid-only liveness off Hyprland), marks every dead
  # session `done`, and prunes it — re-staging graph.json atomically only when
  # something actually changed. One hyprctl call + a stage read/write; it never
  # exits non-zero on "nothing to reap".
  #
  # Seam: this is the OUT-OF-BAND cleanup path for sessions whose IN-BAND
  # cleanup (do_session_end on normal exit) could not run. It is gated with the
  # rest of shellbridge on the quickshell facet, ordered into the graphical
  # session so it inherits HYPRLAND_INSTANCE_SIGNATURE (the compositor imports
  # its env into the user manager), and shares shellbridge's exact AOIDE_STAGE_DIR.
  systemd.user.services.aoide-graph-reap = {
    description = "Aoide graph reaper — resolve KILLED sessions (SUPER+Q/SIGKILL) that could not self-clean";

    after = [ "graphical-session.target" ];
    partOf = [ "graphical-session.target" ];

    # The reaper gathers live window addresses via `hyprctl clients -j`; like
    # shellbridge it needs hyprctl on PATH (else it silently falls back to
    # pid-only liveness and never sees the window-gone signal). Unit-level option.
    # libnotify rides along for `notify-send`: a sweep that actually changed the
    # roster raises a toast through dunst (a quiet sweep stays silent), and a
    # unit PATH without it would degrade that to a journal line nobody reads.
    path = [
      pkgs.hyprland
      pkgs.libnotify
    ];

    serviceConfig = {
      Type = "oneshot";
      ExecStart = "${pkgs.aoide}/bin/aoide graph reap";

      Environment = [
        # Same stage directory shellbridge writes — the reaper repairs it.
        "AOIDE_STAGE_DIR=%h/Aoide/song/stage"
      ];

      NoNewPrivileges = true;
      StandardOutput = "journal";
      StandardError = "journal";
    };
  };

  systemd.user.timers.aoide-graph-reap = {
    description = "Aoide graph reaper timer — periodic liveness sweep (~12s)";

    wantedBy = [ "graphical-session.target" ];
    partOf = [ "graphical-session.target" ];

    timerConfig = {
      # Cheap and frequent: a killed terminal resolves within ~12s. First sweep
      # 15s after the session comes up (let shellbridge seed the stage first);
      # thereafter every 12s since the previous run finished.
      OnActiveSec = "15s";
      OnUnitActiveSec = "12s";
      AccuracySec = "2s";
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
  #   song/stage/livery.json     — resolved note colours (written by the native livery engine)
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
  #       "startedAt":     "ISO-8601 timestamp",
  #       # --- additive v0-safe fields (absent on legacy/shell records) ---
  #       "kind":          "string? — 'agent' | 'shell' | 'subagent'",
  #       "parentSessionId": "string? — spawned-by edge (subagent → parent)",
  #       "title":         "string? — session name (custom-title / graph-send)",
  #       "activity":      "string? — current tool/command being run",
  #       "say":           "string? — agent's latest words (transcript tail)",
  #       "tool":          "string? — agent's latest tool call, 'Name: subject' (transcript tail); unlike `activity` it outlives the turn",
  #       "model":         "string? — active Claude model id, e.g. 'claude-sonnet-5'",
  #       "workspace":     "int?    — Hyprland workspace id of the window",
  #       "pid":           "int?    — lifecycle-owning process pid",
  #       "conductable":   "bool?   — spawned under `aoide conduct`",
  #       "socket":        "string? — per-session injection socket path",
  #       "contextTokens": "int?    — context-window fill of the last request (input+cache tokens); absent until the first assistant turn",
  #       "contextCeiling": "int?   — context-window ceiling for the current model; re-derived on model change",
  #       "needsSudo":     "bool?   — true while a conducted shell is blocked at a sudo prompt; cleared (not set false) once the prompt clears",
  #       "restore":       "object? — a conducted shell's continuously-captured restore snapshot: { cwd, idle, argv, typed }; absent for non-shell sessions"
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
