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
{ config, lib, pkgs, ... }:

lib.mkIf config.aoide.enable {

  # ── Runtime directories ──────────────────────────────────────────────────
  # song/stage/ is the atomic JSON state tree (CONTRACTS.md §4). Created at
  # runtime, never committed, never imported by any module (checks.no-song-read).
  systemd.user.tmpfiles.rules = [
    "d %h/Aoide/log      0700 - - -"
    "d %h/Aoide/song/stage 0755 - - -"
  ];

  # ── aoided systemd user service ──────────────────────────────────────────
  systemd.user.services.aoided = {
    description = "Aoide orchestrator daemon — neutral event stream + policy + audit";

    # Start when the graphical session is ready (Hyprland/Wayland compositor up).
    wantedBy = [ "graphical-session.target" ];
    after    = [ "graphical-session.target" ];
    partOf   = [ "graphical-session.target" ];

    serviceConfig = {
      # The `aoide` package installs both the `aoide` CLI and the `aoided`
      # daemon binary. We reference it via pkgs so this evaluates cleanly
      # even when Agent B hasn't yet realised the package.
      ExecStart = "${pkgs.aoide}/bin/aoided";

      # Restart on failure; don't restart on clean exit or user stop.
      Restart    = "on-failure";
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
      StandardError  = "journal";
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
    after    = [ "aoided.service" ];
    bindsTo  = [ "aoided.service" ];

    serviceConfig = {
      ExecStart  = "${pkgs.aoide}/bin/aoide mcp serve --stdio";
      Restart    = "on-failure";
      RestartSec = "5s";
      Environment = [
        "AOIDE_AUDIT_LOG=${config.aoide.auditLog}"
        "AOIDE_USER=${config.aoide.user}"
      ];
      NoNewPrivileges = true;
      StandardOutput  = "journal";
      StandardError   = "journal";
    };
  };
}
