# modules/dendrites/aoided.nix — the AoideOS deltas for the aoided unit.
#
# Deselecting this dendrite does not remove the aoided daemon: `aoided.service`,
# its tmpfiles rules and the core session variables come from the core module
# `pkgs/aoide/module/aoided.nix`, whose unit is on whenever `aoide.enable` is
# set. This dendrite sets it, and so does every other catalogue member of
# `nucleus`; the core's own switch is never written by hand.
#
# What this dendrite carries is the AoideOS deployment on top of that unit:
# the a2a, mcp and mail-adapter doors, the pair-watch popup, the usage poller
# and its timer, the discovery firewall carve (UDP 8711) and
# `AOIDE_SONG_TEMPLATES`.
#
# `pkgs/aoide/module/aoided.nix` owns the tmpfiles rules, the core session
# variables, and the `aoided.service` unit itself (portable, nixpkgs-only).
# This file carries only what is paint-dependent: the lyra-gated
# `AOIDE_SONG_TEMPLATES` — as a session variable for interactive shells AND on
# the unit's own `Environment`, so a process aoided spawns resolves this
# build's templates rather than whatever the login session was handed. `aoide.sessionTarget` — the seam
# the core unit anchors through — is NOT set here: the lane that brings a
# graphical session up sets it, so this file reads no lane's option (root
# AGENTS.md house rule 5). Below that, the doors (mcp, a2a, pair-watch), the
# discovery-advertisement firewall carve, and the usage widget poller — core
# *binaries* in a still-AoideOS *deployment* (migration brief §2.6 item 4).
#
# The doors' settings (`aoide.mcp`, `aoide.a2a`, `aoide.mail.adapter`,
# `aoide.usage`) are declared below, beside the units that read them, so they
# exist only while this dendrite is selected.
{
  config,
  lib,
  pkgs,
  aoideInputs,
  ...
}:
let
  inherit (lib) mkOption types;
in
{
  options.aoide = {
    # ── Agent control plane ────────────────────────────────────────────────
    mcp.enable = mkOption {
      type = types.bool;
      default = false;
      description = ''
        Enable the MCP façade. Off by default (house policy): agents spawn
        `aoide mcp serve --stdio` per session. Network MCP is user-only and
        never agent-enabled.
      '';
    };

    # ── A2A door (CONTRACTS.md §6, v0) ────────────────────────────────────────
    a2a = {
      enable = mkOption {
        type = types.bool;
        default = false;
        description = ''
          Enable the A2A (Agent2Agent) door. Off by default (house policy),
          same as the MCP façade. Localhost/user-scoped; forwarded A2A
          messages are untrusted data.
        '';
      };

      bindAddress = mkOption {
        type = types.str;
        default = "127.0.0.1";
        description = ''
          The A2A HTTP bind address. Loopback by default; networked A2A is a
          deliberate, user-only choice.
        '';
      };

      port = mkOption {
        type = types.port;
        default = 8710;
        description = "The A2A HTTP port.";
      };

      spawnAgent = mkOption {
        type = types.str;
        default = "";
        description = ''
          The command `a2a serve` conducts for an A2A-spawned task
          (CONTRACTS.md §6, `message/send`'s spawn path). The A2A client
          supplies only the prompt/message, NEVER the command — the
          executable always comes from this option, set by the operator at
          rebuild time (the admission). Empty (the default) disables
          spawning entirely: `message/send` returns a structured error
          instead of launching anything. A bare-word command must also
          appear on the unit's PATH via `spawnPath` below — a systemd user
          unit's default PATH carries none of the system profile.
        '';
      };

      spawnPath = mkOption {
        type = types.listOf types.package;
        default = [ ];
        description = ''
          Packages placed on the aoide-a2a unit's PATH so `spawnAgent`
          (and the conducted child it launches) can resolve by bare name.
          A systemd user unit's default PATH is minimal (coreutils and
          friends) and does not include /run/current-system/sw/bin — found
          live when a door-summoned spawn on a headless host failed with
          "failed to conduct `claude`: No such file or directory" while an
          interactive shell resolved it fine. Same explicit-package rule
          shellbridge and the secrets popup watcher already follow.
        '';
      };

      tokenFile = mkOption {
        type = types.str;
        default = "";
        description = ''
          Path to a file holding the shared secret an inbound `message/send`
          must present (`Authorization: Bearer <token>`) to be trusted
          (CONTRACTS.md §6 amendment, 2026-08-18). A configured file that is
          missing, unreadable or empty refuses every bearer check (the door
          fails closed; it never reads as "no token"). Empty (the default) is
          the fully-open behavior every prior release shipped: loopback
          auto-delivers, Spawn is gated only by `spawnAgent` being set. Once
          non-empty, TWO things change together, with no separate opt-out:
          Spawn REQUIRES a valid token, and loopback STOPS being an implicit
          trust signal (closing the gap where a reverse proxy or tunnel
          makes a remote caller look loopback to the door). The file itself
          is never read by nix — only its path crosses this option and the
          `aoide-a2a` unit's environment; the secret is read off disk once,
          at `a2a serve` launch.
        '';
      };

      bearerSecret = mkOption {
        type = types.str;
        default = "";
        description = ''
          Name of a secret in the local `aoide secrets` broker the A2A door
          expects as its inbound `Authorization: Bearer` token. Only the
          NAME crosses this option and the `aoide-a2a` unit's environment
          (`AOIDE_A2A_BEARER_SECRET`); the door resolves the value fresh on
          every request through the broker socket (consumer `a2a-door`), and
          a resolve failure fails closed. Takes precedence over `tokenFile`
          when set. Empty (the default) leaves the door on the `tokenFile`
          behavior. The unit's user must be able to reach the broker socket
          (group `aoide-secrets-access`).
        '';
      };

      discoveryAdvertise = mkOption {
        type = types.bool;
        default = false;
        description = ''
          Force this instance's own LAN discovery advertisement on (P-P6 +
          task #120, docs/architecture/PAIRING.md's "Discovery
          (advertise-but-locked)" section): `a2a serve` sends a one-line
          `{v, name, host, user}` UDP broadcast advertisement (name plus
          its ssh hop claim — never a door URL, never a key or
          fingerprint: rendezvous, not authentication) on a fixed port,
          ~30s jittered cadence, for `aoide node discover`/`aoide
          pair`'s hostname arm (and bare `aoide pair`) on the same LAN to
          hear. Off by default, same house policy as every other A2A
          knob above; the runtime switch beside this declarative force is
          `aoide node advertise on|off`. Discovery only ever feeds `node
          discover`'s table and `pair`'s hostname-target resolution;
          the pairing ceremony above remains the ONLY thing that ever
          writes a node record.
        '';
      };

      # ── Pairing-ceremony popup (P-PV3, task #132) ─────────────────────────
      pairingPopup = mkOption {
        type = types.bool;
        default = false;
        description = ''
          Enable the `aoide-pair-watch.service` graphical-session user unit
          (`aoide pair watch --popup`) — a typed-code entry dialog per
          actionable pairing request, `lyra pair ask` when it resolves
          (the same six-boxes-plus-dash surface `lyra secrets ask` renders),
          `zenity --entry` otherwise. Off by default, same house policy as
          the MCP façade/A2A door/usage poller/secrets broker: the unit is
          graphical-session-gated exactly like `aoide-secrets-watch`
          (`aoide.a2a.enable && aoide.sessionTarget ==
          "graphical-session.target"`), but this
          flag is the deliberate opt-in ON TOP of that gate — a host with
          a2a on and a graphical session does NOT get the popup unless it
          also sets this. It lives in the `a2a` family because it
          is meaningless without the door (`aoide.a2a.enable` is part of
          its gate). See `modules/dendrites/aoided.nix` for the unit and
          CONTRACTS.md §6's "Pairing events feed" subsection for the
          popup's own typed-code contract.
        '';
      };
    };

    # ── Mail adapter (H1, CONTRACTS.md §6 "aoide mail serve") ────────────────
    mail.adapter = {
      enable = mkOption {
        type = types.bool;
        default = false;
        description = ''
          Run `aoide mail serve` as the `aoide-mail-adapter` user unit: the
          mail-only listener (`aoide/mailDeposit`, `aoide/mailPoll`,
          `aoide/binding` and the stripped AgentCard, no other method) a
          TLS-terminating front points at. Off by default, same house policy
          as the A2A door. Turn it on only on the relay of a mesh. It binds
          127.0.0.1 and nothing else, and there is deliberately no bind
          option beside `port`: the front (a Cloudflare Tunnel, a reverse
          proxy) is a transport hop, and the A2A door is never fronted by one.
        '';
      };

      port = mkOption {
        type = types.port;
        default = 8712;
        description = "The adapter's loopback port: the one the front's ingress targets.";
      };
    };

    # ── Usage widget + poller (opt-in, off by default) ───────────────────────
    usage = {
      enable = mkOption {
        type = types.bool;
        default = false;
        description = ''
          Enable the claude.ai usage widget + poller (`aoide usage`, a
          systemd.user timer running it on `aoide.usage.interval`). Off by
          default, same house policy as the MCP façade and A2A door.
        '';
      };

      interval = mkOption {
        type = types.str;
        default = "300s";
        description = ''
          Poller cadence (systemd `OnUnitActiveSec` duration) for `aoide
          usage`, which writes `state/usage.json` (CONTRACTS.md §4): the
          local token/cost rollup from this machine's own transcripts, plus
          the live claude.ai block the unit spawns `curl` for. The widget
          calls a document stale at three times this cadence.
        '';
      };
    };
  };

  config = {
    aoide.enable = true;

    # ── AoideOS-only session variable ────────────────────────────────────────
    # `AOIDE_SONG_TEMPLATES` (L-C3, task #107) is paint data (the shipped
    # songbook), not core state like `AOIDE_ROOT`/`AOIDE_FLAKE_ROOT`
    # (`pkgs/aoide/module/aoided.nix`'s own contract) — gated on
    # `aoide.lyra.enable`, the option that actually controls whether the
    # `lyra` binary is installed on this host at all (review finding: a
    # headless-core box that never installs `lyra` has no reader for this
    # var, so exporting it unconditionally only drags the
    # `pkgs.lyra-songbook` closure onto every interactive shell for no
    # reason). An operator's own `lyra rice compose` still needs it — the
    # sibling-of-binary fallback tier (`fs::song_templates_dir`'s own doc)
    # can't resolve across `lyra`'s separate `rice` output, so this env
    # export is the ONLY way an interactive shell agrees with `lyra` on
    # where the templates are.
    environment.sessionVariables = lib.optionalAttrs config.aoide.lyra.enable {
      AOIDE_SONG_TEMPLATES = "${pkgs.lyra-songbook}/share/lyra/songbook";
    };

    # ── The same variable on the unit itself ─────────────────────────────────
    # The session variable above is a LOGIN fact: it lands in the systemd user
    # manager's environment when the operator logs in and stays there, so a unit
    # that started before a `nixos-rebuild switch` keeps the PREVIOUS build's
    # templates until they relog (found at the S9 yomi switch). Any process
    # aoided spawns — a conducted session, an agent, a terminal — inherits the
    # unit's environment, so a `lyra rice stage` run from one of them resolved
    # the old songbook with nothing saying so. Declared here, a unit restart is
    # enough, and the value always names THIS build's `pkgs.lyra-songbook`.
    #
    # Why here and not in the portable core module: the templates path is
    # `pkgs.lyra-songbook` (paint), which core neither knows nor may name. Why
    # only this unit, `aoide-quickshell` and `aoide-rice-reload`:
    # `fs::song_templates_dir` is read on the staging path (`rice stage|compose|
    # preview|mode`, `take`, `onboard`), and those are what a shell born of
    # `aoide-quickshell` runs and what `aoide-rice-reload` re-stages with at
    # login and after a switch — shellbridge already spells it for the one
    # process that stages in-process, and a door that spawns a fixed dialog
    # (`lyra pair ask`, `lyra secrets ask`) never reaches the staging path at
    # all. The rest of what the staging path reads
    # from session variables was audited with it: `AOIDE_ROOT` and
    # `AOIDE_FLAKE_ROOT` are spelled by every unit that can reach it,
    # `AOIDE_STAGE_DIR` is deliberately unset anywhere (the default layout
    # resolves under `AOIDE_ROOT`), and `XDG_RUNTIME_DIR` is the manager's own.
    systemd.user.services.aoided.serviceConfig.Environment =
      lib.optional config.aoide.lyra.enable "AOIDE_SONG_TEMPLATES=${pkgs.lyra-songbook}/share/lyra/songbook";

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
    # decides per tick — this flag OR the runtime `aoide node advertise on`
    # switch (state/advertise.json) makes it send a one-line UDP BROADCAST
    # advertisement to 255.255.255.255:8711 (wire v2: name + ssh hop claim
    # {host, user}, never a URL, key, or credential) every ~30s so `aoide
    # node discover`/`aoide pair`'s hostname arm on other boxes can hear this
    # instance —
    # discovery grants nothing by itself, the pairing ceremony above is
    # still the only thing that ever writes a node record.
    systemd.user.services.aoide-a2a = lib.mkIf config.aoide.a2a.enable {
      description = "Aoide A2A (Agent2Agent) door (loopback by default, user-only)";

      wantedBy = [ "aoided.service" ];
      after = [ "aoided.service" ];
      bindsTo = [ "aoided.service" ];

      # The spawn agent (and everything the conducted child execs) resolves
      # off THIS unit's PATH, not the operator's shell — `aoide.a2a.spawnPath`
      # names the packages explicitly (see the option's own live-incident
      # note; empty when spawning is disabled). openssh rides alongside because
      # the door drains the outbox after every deposit (`spool_and_drain_ack`)
      # and the tunnel spawns `ssh` by bare name — same gap the aoided unit closed.
      path = [ pkgs.openssh ] ++ config.aoide.a2a.spawnPath;

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

    # ── Mail adapter (H1, opt-in, off by default per house policy) ───────────
    # When aoide.mail.adapter.enable is true, run `aoide mail serve`: the
    # loopback, mail-only listener a TLS-terminating front (cloudflared, a
    # reverse proxy) points at on the relay of a mesh. Ordered and restarted
    # like aoide-a2a, whose sibling it is. There is no bind setting to pass:
    # the binary binds 127.0.0.1 and nothing else (CONTRACTS.md §6), and the
    # front's ingress targets `aoide.mail.adapter.port`, never `aoide.a2a.port`.
    # `openssh` and `curl` ride the unit's `path` because a transit deposit
    # drains toward the next hop from inside this process, the same gap the
    # aoided unit closed.
    systemd.user.services.aoide-mail-adapter = lib.mkIf config.aoide.mail.adapter.enable {
      description = "Aoide mail adapter (H1: loopback, mail-only, behind a TLS-terminating front)";

      wantedBy = [ "aoided.service" ];
      after = [ "aoided.service" ];
      bindsTo = [ "aoided.service" ];

      path = [
        pkgs.openssh
        pkgs.curl
      ];

      serviceConfig = {
        ExecStart = "${pkgs.aoide}/bin/aoide mail serve";
        Restart = "on-failure";
        RestartSec = "5s";
        Environment = [
          "AOIDE_MAIL_ADAPTER_PORT=${toString config.aoide.mail.adapter.port}"
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
    # Gated on `aoide.a2a.enable` plus `aoide.a2a.pairingPopup` (both DEFAULT
    # FALSE, declared in this file's options block): each is an opt-in
    # sub-feature switch, so no host gets this popup without asking for it. Never flipped on here; deployment flips are the User's.
    #
    # Deliberately NOT gated on "a graphical session exists" (the condition its
    # sibling `aoide-secrets-watch` gates on, `aoide.sessionTarget ==
    # "graphical-session.target"`). What this unit needs is a graphical session
    # AND A DIALOG BINARY, and the session target is neither — it is the anchor
    # one painting lane happened to pick. A host whose desktop is painted by
    # something else — osaka, running core Aoide beside dxflake's own Hyprland
    # and Stylix — has the session but no such anchor, gets zenity from the
    # `path` below, and `pair_watch` itself only refuses when NEITHER `lyra`
    # nor `zenity` resolves. Gating on the session target would have made the
    # popup structurally unreachable there for a reason that has nothing to do
    # with pairing.
    systemd.user.services.aoide-pair-watch =
      lib.mkIf (config.aoide.a2a.enable && config.aoide.a2a.pairingPopup)
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
          # `libnotify` provides `notify-send`, resolved the SAME bare-name way
          # for the reply-code toast (`pair_watch::notify_reply_code`).
          path = [
            pkgs.zenity
            pkgs.libnotify
          ]
          ++
            lib.optional config.aoide.lyra.enable
              aoideInputs.quickshell.packages.${pkgs.stdenv.hostPlatform.system}.default;

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
    # KNOWN ASYMMETRY: the runtime switch (`aoide node advertise on`,
    # state/advertise.json) turns SENDING on without nix — but this firewall
    # carve hangs only off the nix flag, so a box advertising via the runtime
    # switch alone still has its own `node discover` sweeps firewalled (and
    # never hears itself). A box that only RECEIVES (`node discover`/`node
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
  };
}
