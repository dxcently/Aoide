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
  inputs,
  ...
}:

lib.mkIf config.aoide.enable {

  # ── Runtime directories ──────────────────────────────────────────────────
  # Two atomic JSON state trees (CONTRACTS.md §4), both created at runtime,
  # never committed, never imported by any module (checks.no-song-read):
  # `song/stage/` is rice/paint staging (livery.json, mode.json — lyra's
  # tree); `state/stage/` is CONDUCTING state (sessions.json/hooks.json/
  # projects.json/graph.json/pending.json/herald.json — command-defrag S1,
  # 2026-08-27). `state/stage/` nests under the already-0700 `state/` dir, so
  # its own 0755 grants nothing beyond the owner the parent doesn't already
  # gate — matching `song/stage/`'s own mode rather than inventing a second
  # convention for the same class of data.
  systemd.user.tmpfiles.rules = [
    "d ${config.aoide.root}/log        0700 - - -"
    "d ${config.aoide.root}/song/stage 0755 - - -"
    "d ${config.aoide.root}/state      0700 - - -"
    "d ${config.aoide.root}/state/stage 0755 - - -"
  ];

  # ── The terminal, for the interactive half ───────────────────────────────
  # See the unit's own `Environment` note below: `spawn --windowed` and
  # `resurrect` are ordinary commands an operator runs in a shell, and a
  # shell inherits this no more than a systemd unit does. One option, two
  # consumers.
  environment.sessionVariables =
    (lib.optionalAttrs (config.aoide.terminal != "") { AOIDE_TERMINAL = config.aoide.terminal; })
    # `AOIDE_SONG_TEMPLATES` (L-C3, task #107) is paint data (the shipped
    # songbook), not core state like `AOIDE_ROOT`/`AOIDE_FLAKE_ROOT` below —
    # gated on `aoide.lyra.enable`, the option that actually controls
    # whether the `lyra` binary is installed on this host at all (review
    # finding: a headless-core box that never installs `lyra` has no reader
    # for this var, so exporting it unconditionally only drags the
    # `pkgs.lyra-songbook` closure onto every interactive shell for no
    # reason). An operator's own `lyra rice compose` still needs it — the
    # sibling-of-binary fallback tier (`fs::song_templates_dir`'s own doc)
    # can't resolve across `lyra`'s separate `rice` output, so this env
    # export is the ONLY way an interactive shell agrees with `lyra` on
    # where the templates are.
    // (lib.optionalAttrs config.aoide.lyra.enable {
      AOIDE_SONG_TEMPLATES = "${pkgs.lyra-songbook}/share/lyra/songbook";
    })
    // {
      # Non-default values only need to WORK; the defaults are chosen so
      # that unset == set-to-default already matches core's own code
      # default (L-C2, task #107) — exported anyway so an interactive shell
      # agrees with every unit above on where the runtime root/checkout
      # sit, the same "one option, two consumers" shape `AOIDE_TERMINAL`
      # already holds.
      AOIDE_ROOT = config.aoide.root;
      AOIDE_FLAKE_ROOT = config.aoide.checkout;
    };

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
      #
      # `AOIDE_TERMINAL` rides here for the same reason the audit log does,
      # and is load-bearing rather than convenience: the boot auto-resume
      # sweep opens REAL terminals, and a systemd user unit inherits no
      # shell environment, so without this entry the sweep runs, selects its
      # carried sessions, and fails every windowed spawn with the taught
      # no-terminal error — resuming nothing while looking healthy. Omitted
      # entirely when `aoide.terminal` is empty, since the resolver reads
      # "set but blank" as a configured template and would answer with a
      # confusing parse instead of that taught error.
      Environment = [
        "AOIDE_AUDIT_LOG=${config.aoide.auditLog}"
        "AOIDE_USER=${config.aoide.user}"
        "AOIDE_ROOT=${config.aoide.root}"
        "AOIDE_FLAKE_ROOT=${config.aoide.checkout}"
      ]
      # Quoted as one assignment per systemd.exec(5)'s own `Environment=`
      # syntax (`Environment="VAR=word1 word2"`): the template carries
      # spaces (e.g. `kitty -e {cmd}`), and an unquoted assignment is
      # whitespace-split into separate tokens, silently dropping everything
      # after the first word as an invalid assignment.
      ++ lib.optional (config.aoide.terminal != "") ''"AOIDE_TERMINAL=${config.aoide.terminal}"'';
      # The interactive half of the same need: `spawn --windowed` and
      # `resurrect` are ordinary commands an operator runs in a shell,
      # and a shell has no more of this variable than the unit does. The unit
      # entry above and this export are the two consumers of one option; a
      # box configured for the daemon but not the shell would answer the
      # taught no-terminal error to the operator while resuming fine at boot.

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
        "AOIDE_ROOT=${config.aoide.root}"
        "AOIDE_FLAKE_ROOT=${config.aoide.checkout}"
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
  # discoveryAdvertise` (P-P6 + task #120, docs/architecture/PAIRING.md's
  # "Discovery (advertise-but-locked)" section) FORCES advertising on for
  # this process: the advertise thread always runs inside `a2a serve` and
  # decides per tick — this flag OR the runtime `aoide peer advertise on`
  # switch (state/advertise.json) makes it send a one-line UDP BROADCAST
  # advertisement to 255.255.255.255:8711 (wire v2: name + ssh hop claim
  # {host, user}, never a URL, key, or credential) every ~30s so `aoide
  # peer discover`/`aoide pair`'s hostname arm on other boxes can hear this
  # instance —
  # discovery grants nothing by itself, the pairing ceremony above is
  # still the only thing that ever writes a peer record.
  systemd.user.services.aoide-a2a = lib.mkIf config.aoide.a2a.enable {
    description = "Aoide A2A (Agent2Agent) door (loopback by default, user-only)";

    wantedBy = [ "aoided.service" ];
    after = [ "aoided.service" ];
    bindsTo = [ "aoided.service" ];

    # The spawn agent (and everything the conducted child execs) resolves
    # off THIS unit's PATH, not the operator's shell — `aoide.a2a.spawnPath`
    # names the packages explicitly (see the option's own live-incident
    # note; empty when spawning is disabled).
    path = config.aoide.a2a.spawnPath;

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
        "AOIDE_ROOT=${config.aoide.root}"
        "AOIDE_FLAKE_ROOT=${config.aoide.checkout}"
      ];
      NoNewPrivileges = true;
      StandardOutput = "journal";
      StandardError = "journal";
    };
  };

  # ── Pairing events watcher (P-P5; popup upgraded to a typed-code entry
  # dialog + lyra/zenity feature-detection, and gated behind its own opt-in
  # flag, at P-PV3 task #132) ────────────────────────────────────────────────
  # Surfaces the pairing ceremony's own events feed (`aoide pair watch
  # --popup`, CONTRACTS.md §6's "Pairing events feed" subsection) as a
  # typed-code entry dialog per actionable request — the same unit shape
  # `secrets.nix`'s own `aoide-secrets-watch` holds (graphical-session.target,
  # `Type = simple` + `Restart = on-failure`, explicit `zenity`/`quickshell`
  # `path` entries, journal both streams, NoNewPrivileges), including that
  # unit's own `lyra`-preferred-with-zenity-fallback shape: `lyra pair ask`
  # (`pair_watch::resolve_lyra_bin`) when `AOIDE_RICE_BIN` resolves it, else
  # `zenity --entry`. No ordering against `aoide-a2a` (the process that
  # actually emits onto this feed) — `pair_watch::wait_for_follower` already
  # retries until the events feed appears rather than exiting 1, so this
  # unit starting before `a2a serve` has bound its socket, or before
  # `aoided` itself has created the feed file, is a normal, harmless race,
  # the same reasoning `aoide-secrets-watch` already holds against the
  # SYSTEM-unit `aoide-secrets-serve` above.
  #
  # Gated on `aoide.a2a.enable` plus `aoide.a2a.pairingPopup` (DEFAULT
  # FALSE, `options.nix`) — modules' own "flags default off" house rule is
  # carried by the opt-in flag itself: no host gets this popup without
  # asking for it. Never flipped on here; deployment flips are the User's.
  #
  # Deliberately NOT gated on `aoide.facets.quickshell.enable`, unlike its
  # sibling `aoide-secrets-watch`. What this unit needs is a graphical
  # session and A DIALOG BINARY, and the facet is neither: it is Aoide's own
  # shell (bar, dock, notifications). A host whose desktop is painted by
  # something else — osaka, running core Aoide beside dxflake's own Hyprland
  # and Stylix — has the session and gets zenity from the `path` below, and
  # `pair_watch` itself only refuses when NEITHER `lyra` nor `zenity`
  # resolves. Gating on the facet would have made the popup structurally
  # unreachable there for a reason that has nothing to do with pairing.
  systemd.user.services.aoide-pair-watch =
    lib.mkIf
      (config.aoide.a2a.enable && config.aoide.a2a.pairingPopup)
      {
        description = "Aoide pairing-ceremony popup watcher — surfaces actionable pairing requests as a typed-code entry dialog";

        wantedBy = [ "graphical-session.target" ];
        after = [ "graphical-session.target" ];
        partOf = [ "graphical-session.target" ];

        # `zenity` must resolve off a bare-name `PATH` lookup
        # (`pair_watch::zenity_available`/`spawn_zenity_entry`, both take the
        # binary NAME, never a hardcoded path) — the same PATH gap
        # `aoide-secrets-watch` documents in `secrets.nix`. It rides
        # unconditionally: it is the fallback the whole unit's reachability
        # rests on, and the only dialog a host without lyra ever gets.
        #
        # `quickshell` rides the SAME condition as `AOIDE_RICE_BIN` below,
        # and for the same one reason: `lyra pair ask` spawns quickshell by
        # bare name, so the unit that can spawn lyra must carry it — and the
        # unit can only spawn lyra when `aoide.lyra.enable` put the binary
        # there. A host without lyra takes the zenity path and has no use
        # for quickshell in its closure.
        path = [
          pkgs.zenity
        ]
        ++ lib.optional config.aoide.lyra.enable
          inputs.quickshell.packages.${pkgs.stdenv.hostPlatform.system}.default;

        serviceConfig = {
          Type = "simple";
          Restart = "on-failure";
          RestartSec = "3s";

          ExecStart = "${pkgs.aoide}/bin/aoide pair watch --popup";

          Environment = [
            "AOIDE_ROOT=${config.aoide.root}"
            "AOIDE_FLAKE_ROOT=${config.aoide.checkout}"
            "AOIDE_USER=${config.aoide.user}"
          ]
          # `pair_watch::resolve_lyra_bin` resolves `lyra` via
          # `AOIDE_RICE_BIN` (tier 1, trusted unconditionally) or a sibling
          # of `current_exe()` (tier 2) — `lyra` ships from `pkgs.aoide.rice`,
          # a SEPARATE output from the `aoide` binary this unit execs, so
          # sibling resolution would silently fail here without this. Only
          # set when `aoide.lyra.enable` is actually on — mirrors
          # `aoide-secrets-watch`'s own identical guard.
          ++ lib.optional config.aoide.lyra.enable "AOIDE_RICE_BIN=${pkgs.aoide.rice}/bin/lyra";

          NoNewPrivileges = true;
          StandardOutput = "journal";
          StandardError = "journal";
        };
      };

  # ── Discovery advertisement firewall (task #98 diagnosis, #120 wire) ─────
  # The advertise thread inside `aoide-a2a` sends its LAN advertisement as
  # plain UDP BROADCAST to 255.255.255.255:8711
  # (`aoide_storage::advertise::PORT`, CONTRACTS.md §6's "Discovery
  # advertisement" subsection — broadcast replaced the original multicast
  # group after the LAN's router ate cross-box multicast, task #106).
  # NixOS's default firewall trusts only `lo` and drops every other inbound
  # packet that isn't part of an established connection — including this
  # SAME host's own broadcast arriving back on its physical interface (a
  # broadcast always self-loops). Confirmed live on yomi-strix (2026-08-26,
  # multicast era; the dst-port match carries over unchanged): raw UDP over
  # `lo` succeeds, the identical send over `eno1` is silently dropped
  # without this rule. `bindAddress` above stays loopback-by-default and
  # unmanaged here on purpose (going non-loopback is a deliberate, separate
  # operator choice); the advertisement is different in kind —
  # `discoveryAdvertise` IS the deliberate opt-in for this instance to be
  # found on the LAN, so flipping it also opens the one port that opt-in
  # requires, the same "one option changes two things together" shape
  # `tokenFile`/`bearerSecret` already hold in options.nix.
  # KNOWN ASYMMETRY: the runtime switch (`aoide peer advertise on`,
  # state/advertise.json) turns SENDING on without nix — but this firewall
  # carve hangs only off the nix flag, so a box advertising via the runtime
  # switch alone still has its own `peer discover` sweeps firewalled (and
  # never hears itself). A box that only RECEIVES (`peer discover`/`peer
  # invite`, both toggles off) likewise opens this port by hand — there is
  # no persistent "this box discovers" state to hang a firewall rule off.
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
        "AOIDE_ROOT=${config.aoide.root}"
        "AOIDE_FLAKE_ROOT=${config.aoide.checkout}"
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
