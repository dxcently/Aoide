# modules/nucleus/aoided.nix — the orchestrator daemon service.
#
# aoided is the central event bus and policy enforcer of the Aoide framework:
#
#   - Emits a neutral event stream consumed by thin per-agent adapters.
#   - Subscriptions are default-deny per event class (OSD flood → no agent run).
#   - Policy, lint, and the single audit log (aoide.auditLog) live here.
#   - Owns the user-gated rebuild pipeline (polkit pattern):
#       agent proposes → user admits → git records.
#   - Both the CLI door and the MCP door write to the same audit log;
#     there is no separate per-door log (concepts/Governance).
#
# Security boundary: forwarded notification text is untrusted input. Adapters
# must wrap it as data and never execute it as a command. This is enforced by
# the adapter pattern (modules/nucleus/melete-adapter.nix), not by aoided
# itself — but aoided's subscription-class gating is the structural backstop.
#
# The `aoide` binary (installed by pkgs/aoide, Agent B) ships both `aoide` (CLI)
# and `aoided` (daemon). We reference it via pkgs so eval stays clean even
# before the package is fully realised.
{
  config,
  lib,
  pkgs,
  ...
}:

lib.mkIf config.aoide.enable {

  # ── Runtime directories ──────────────────────────────────────────────────
  # song/stage/ is the atomic JSON state tree (CONTRACTS.md §4). Created at
  # runtime, never committed, never imported by any module (checks.no-song-read).
  systemd.user.tmpfiles.rules = [
    "d %h/Aoide/log      0700 - - -"
    "d %h/Aoide/song/stage 0755 - - -"
    "d %h/Aoide/state    0700 - - -"
  ];

  # ── aoided systemd user service ──────────────────────────────────────────
  systemd.user.services.aoided = {
    description = "Aoide orchestrator daemon — neutral event stream + policy + audit";

    # On a painting box, start when the graphical session is ready (compositor
    # up — the daemon serves the shell, and partOf ties its lifetime to the
    # session). Headless (quickshell facet off) there is no graphical-session
    # target to anchor to — PartOf then propagates an immediate stop to a
    # manually started daemon, and BindsTo drags the a2a/mcp doors down with
    # it (found live on sakaki). Anchor to default.target instead: with
    # linger on, the daemon and its doors come up at boot and stay resident.
    wantedBy = [
      (if config.aoide.facets.quickshell.enable then "graphical-session.target" else "default.target")
    ];
    after = lib.optional config.aoide.facets.quickshell.enable "graphical-session.target";
    partOf = lib.optional config.aoide.facets.quickshell.enable "graphical-session.target";

    serviceConfig = {
      # The `aoide` package installs both the `aoide` CLI and the `aoided`
      # daemon binary. We reference it via pkgs so this evaluates cleanly
      # even when Agent B hasn't yet realised the package.
      ExecStart = "${pkgs.aoide}/bin/aoided";

      # Restart on failure; don't restart on clean exit or user stop.
      Restart = "on-failure";
      RestartSec = "3s";

      # Audit log path comes from the option contract (modules/nucleus/options.nix).
      # Passed as an environment variable so the daemon picks it up without a
      # secondary config file at this skeleton stage.
      Environment = [
        "AOIDE_AUDIT_LOG=${config.aoide.auditLog}"
        "AOIDE_USER=${config.aoide.user}"
      ];

      # Harden: no new privileges; keep the user session's dbus accessible.
      NoNewPrivileges = true;

      # Standard output goes to the journal for `journalctl --user -u aoided`.
      StandardOutput = "journal";
      StandardError = "journal";
    };

    # Skeleton: a real implementation will also set up the unix socket path,
    # subscription manifest path, and polkit agent address. Those land when
    # Agent B ships the daemon binary; the seams (env vars above) are the
    # real integration points.
  };

  # ── MCP façade (opt-in, off by default per house policy) ────────────────
  # When aoide.mcp.enable is true, also start the per-session MCP server stub.
  # Agents spawn `aoide mcp serve --stdio` themselves; this service is the
  # network-addressable façade for tooling that cannot do stdio transport.
  systemd.user.services.aoide-mcp = lib.mkIf config.aoide.mcp.enable {
    description = "Aoide MCP façade (network, user-only)";

    wantedBy = [ "aoided.service" ];
    after = [ "aoided.service" ];
    bindsTo = [ "aoided.service" ];

    serviceConfig = {
      ExecStart = "${pkgs.aoide}/bin/aoide mcp serve --stdio";
      Restart = "on-failure";
      RestartSec = "5s";
      Environment = [
        "AOIDE_AUDIT_LOG=${config.aoide.auditLog}"
        "AOIDE_USER=${config.aoide.user}"
      ];
      NoNewPrivileges = true;
      StandardOutput = "journal";
      StandardError = "journal";
    };
  };

  # ── A2A façade (opt-in, off by default per house policy) ─────────────────
  # When aoide.a2a.enable is true, start the A2A (Agent2Agent) server: a
  # JSON-RPC/HTTP door (AgentCard + tasks/get + message/send, CONTRACTS.md §6)
  # exposing aoide-orchestrated sessions to other A2A-speaking agents.
  # message/send's spawn path only ever launches `aoide.a2a.spawnAgent` (set
  # below, empty by default = spawning disabled) — never a client-supplied
  # command. Loopback/user-scoped by default — same security posture as
  # aoide-mcp. `aoide.a2a.tokenFile` (empty by default) is the bearer-token
  # gate (CONTRACTS.md §6 amendment, 2026-08-18): once set, Spawn requires a
  # valid token AND loopback stops auto-trusting an unauthenticated caller —
  # the fix for a reverse proxy/tunnel making a remote caller look loopback.
  systemd.user.services.aoide-a2a = lib.mkIf config.aoide.a2a.enable {
    description = "Aoide A2A (Agent2Agent) door (loopback by default, user-only)";

    wantedBy = [ "aoided.service" ];
    after = [ "aoided.service" ];
    bindsTo = [ "aoided.service" ];

    serviceConfig = {
      ExecStart = "${pkgs.aoide}/bin/aoide a2a serve";
      Restart = "on-failure";
      RestartSec = "5s";
      Environment = [
        "AOIDE_A2A_BIND=${config.aoide.a2a.bindAddress}"
        "AOIDE_A2A_PORT=${toString config.aoide.a2a.port}"
        "AOIDE_A2A_SPAWN_AGENT=${config.aoide.a2a.spawnAgent}"
        "AOIDE_A2A_TOKEN_FILE=${config.aoide.a2a.tokenFile}"
        "AOIDE_AUDIT_LOG=${config.aoide.auditLog}"
        "AOIDE_USER=${config.aoide.user}"
      ];
      NoNewPrivileges = true;
      StandardOutput = "journal";
      StandardError = "journal";
    };
  };

  # ── Usage widget poller (opt-in, off by default per house policy) ────────
  # When aoide.usage.enable is true, run `aoide usage` on a timer: it computes
  # the LOCAL token/cost rollup from this machine's own Claude Code
  # transcripts and fetches the LIVE claude.ai usage block (CONTRACTS.md §4),
  # then atomically writes state/usage.json for the widget to read. Oneshot
  # service + timer (not a long-lived process, unlike aoide-mcp/aoide-a2a)
  # since each run is a quick scan-and-write.
  systemd.user.services.aoide-usage = lib.mkIf config.aoide.usage.enable {
    description = "Aoide usage widget: local token/cost rollup + live claude.ai usage fetch";

    # curl MUST be on the unit PATH: the live half of `aoide usage` spawns
    # `curl` for the /api/oauth/usage fetch, and a user unit's default PATH
    # (coreutils &c.) lacks it — without this every tick degrades the live
    # block to {error: "curl failed"}. Unit-level option, not serviceConfig.
    path = [ pkgs.curl ];

    serviceConfig = {
      Type = "oneshot";
      ExecStart = "${pkgs.aoide}/bin/aoide usage";
      Environment = [
        "AOIDE_STATE_DIR=%h/Aoide/state"
        "AOIDE_USER=${config.aoide.user}"
      ];
      NoNewPrivileges = true;
      StandardOutput = "journal";
      StandardError = "journal";
    };
  };

  systemd.user.timers.aoide-usage = lib.mkIf config.aoide.usage.enable {
    description = "Poll the local usage rollup on a timer";
    wantedBy = [ "timers.target" ];
    timerConfig = {
      OnBootSec = "1m";
      OnUnitActiveSec = config.aoide.usage.interval;
      Unit = "aoide-usage.service";
    };
  };
}
