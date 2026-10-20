# pkgs/aoide/module/reap.nix — core's liveness reaper: a periodic `aoide session reap` that
# judges the sessions a dead terminal left behind. Core owns it, not the desktop, so it is
# its own unit file, anchored like aoided and never gated on Strillix.
{
  config,
  lib,
  pkgs,
  ...
}:

let
  cfg = config.aoide;
in

lib.mkIf config.aoide.enable {
  # ── Liveness reaper: timer + oneshot service ──────────────────────────────
  # A terminal killed with SUPER+Q / SIGKILL is torn down uncatchably, so its `conduct`/`wrap`
  # process never runs its own `session end` and the session stays `running` forever. The sweep
  # is the out-of-band cleanup: a cheap periodic `aoide session reap` (~12s) that marks dead
  # sessions `done` and prunes them, re-staging graph.json atomically only when something
  # changed. Core owns it, not the desktop: the window-closed event reaches it through the door
  # (`session window closed`), and nothing here needs a compositor. The unit name is kept as
  # `aoide-graph-reap` (renaming would churn enabled-unit state at the next switch for nothing).
  systemd.user.services.aoide-graph-reap = {
    description = "Aoide graph reaper — resolve KILLED sessions (SUPER+Q/SIGKILL) that could not self-clean";

    # The same anchor as aoided: `aoide.sessionTarget` decides when the sweep starts.
    after = lib.optional (cfg.sessionTarget != "default.target") cfg.sessionTarget;

    # `notify-send` raises a toast when a sweep changed the roster; `ps` is the desktop-Codex
    # scan (without it that scan can only report Unknown, P-CX-4). Neither puts a compositor in
    # the closure: core execs no `hyprctl`.
    path = [
      pkgs.libnotify
      pkgs.procps
    ];

    serviceConfig = {
      Type = "oneshot";
      ExecStart = "${pkgs.aoide}/bin/aoide session reap";

      Environment = [
        "AOIDE_USER=${config.aoide.user}"
        "AOIDE_ROOT=${config.aoide.root}"
        "AOIDE_FLAKE_ROOT=${config.aoide.checkout}"
      ];

      NoNewPrivileges = true;
      StandardOutput = "journal";
      StandardError = "journal";
    };
  };

  systemd.user.timers.aoide-graph-reap = {
    description = "Aoide graph reaper timer — periodic liveness sweep (~12s)";

    wantedBy = [ cfg.sessionTarget ];

    timerConfig = {
      # First sweep 15s after the session comes up; thereafter every 12s since the previous run finished.
      OnActiveSec = "15s";
      OnUnitActiveSec = "12s";
      AccuracySec = "2s";
    };
  };
}
