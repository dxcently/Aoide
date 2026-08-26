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

      # aoided is the resident daemon (P-D2, docs/architecture/AOIDED.md):
      # `daemon::run_loop` binds its own control socket, spawns the accept
      # loop, and ticks forever — it never exits on its own, so Type=simple
      # is the correct declaration (an EARLIER skeleton build that exited
      # after one policy self-check needed oneshot+RemainAfterExit instead,
      # since a clean exit under Type=simple back then flipped the unit
      # inactive and BindsTo dragged the a2a/mcp doors down with it — found
      # live on osaka). Restart=on-failure covers a crash (a first-loop bug,
      # a bind failure) without masking one as permanently "active".
      Type = "simple";
      Restart = "on-failure";
      RestartSec = "5s";

      # Audit log path comes from the option contract (modules/nucleus/options.nix).
      # Passed as an environment variable so the daemon picks it up without a
      # secondary config file. The control socket and events feed need no
      # entry here — `daemon::socket_path`/`daemon::events_path` default to
      # `$XDG_RUNTIME_DIR/aoide/aoided.sock`/`events.jsonl` (a systemd user
      # unit already has `XDG_RUNTIME_DIR` set); `AOIDE_DAEMON_SOCKET`/
      # `AOIDE_DAEMON_EVENTS` are the override seam for a host that needs
      # something else, not something this unit has to set.
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

    # The control socket, events feed, and registry dispatch are live
    # (P-D2/P-D4, docs/architecture/AOIDED.md) — `ping`/`subscribe` today,
    # `dispatch` (the fourth door) once P-D4 lands. Producers onto the
    # events feed (the secrets-feed mirror, the #69 hand-edit watcher) are
    # P-D3, not yet wired into the tick loop.
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
  # aoide-mcp. Two bearer gates, `aoide.a2a.bearerSecret` taking precedence:
  # a broker-resolved secret NAME (value fetched fresh per request, resolve
  # failure fails closed) or `aoide.a2a.tokenFile`, a path read once at
  # launch. Either, once set, means Spawn requires a valid token AND loopback
  # stops auto-trusting an unauthenticated caller — the fix for a reverse
  # proxy/tunnel making a remote caller look loopback. `aoide.a2a.
  # discoveryAdvertise` (P-P6, docs/architecture/PAIRING.md's "Discovery
  # (advertise-but-locked)" section) is a THIRD, independent off-by-default
  # toggle: it starts a background thread inside this SAME process that
  # sends a one-line LAN multicast beacon (name/fingerprint/url, never a
  # credential) every ~30s so `aoide peer discover`/`peer invite` on other
  # boxes can hear this instance — discovery grants nothing by itself, the
  # pairing ceremony above is still the only thing that ever writes a peer
  # record.
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
        "AOIDE_A2A_BEARER_SECRET=${config.aoide.a2a.bearerSecret}"
        "AOIDE_DISCOVERY_ADVERTISE=${if config.aoide.a2a.discoveryAdvertise then "1" else ""}"
        "AOIDE_AUDIT_LOG=${config.aoide.auditLog}"
        "AOIDE_USER=${config.aoide.user}"
      ];
      NoNewPrivileges = true;
      StandardOutput = "journal";
      StandardError = "journal";
    };
  };

  # ── Discovery beacon firewall (task #98 diagnosis) ────────────────────────
  # `discoveryAdvertise` starts a background thread inside `aoide-a2a` that
  # sends its LAN beacon on a fixed UDP multicast group+port
  # (`aoide_storage::beacon::GROUP`/`PORT`, 239.255.87.10:8711, CONTRACTS.md
  # §6's "Discovery beacon" subsection). NixOS's default firewall trusts only
  # `lo` and drops every other inbound packet that isn't part of an
  # established connection — including a beacon this SAME host's own
  # multicast-loopback delivers to its own listening socket on a real
  # interface, not only a beacon actually arriving from the wire. Confirmed
  # live on yomi-strix (2026-08-26): an independent raw UDP send/receive over
  # `lo` succeeds, the identical send/receive over `eno1` is silently
  # dropped, and `firewall-start`'s generated ruleset shows zero
  # `allowedUDPPorts` for this port — a socket bug was ruled out first
  # (`ip route get 239.255.87.10` already resolves via the right interface,
  # and pinning `IP_MULTICAST_IF` explicitly changed nothing). `bindAddress`
  # above stays loopback-by-default and unmanaged here on purpose (going
  # non-loopback is a deliberate, separate operator choice); the beacon is
  # different in kind — `discoveryAdvertise` IS the deliberate opt-in for
  # this instance to be found on the LAN, so flipping it also opens the one
  # port that opt-in requires, the same "one option changes two things
  # together" shape `tokenFile`/`bearerSecret` already hold in options.nix.
  # A box that only ever wants to RECEIVE (`peer discover`/`peer invite`
  # with `discoveryAdvertise` left off) still needs this port opened by
  # hand — not covered by this toggle, since there is no persistent
  # "this box discovers" state to hang a firewall rule off of.
  networking.firewall.allowedUDPPorts = lib.mkIf config.aoide.a2a.discoveryAdvertise [ 8711 ];

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
