# modules/nucleus/melete-adapter.nix — thin per-agent adapter on the aoided event stream.
#
# This is the concrete exemplar of the adapter pattern described in
# entities/aoided.md and concepts/Desktop-Architecture.md.
#
# Responsibility:
#   - Subscribe (via aoided's subscription manifest) to a narrow set of
#     event classes relevant to Melete job dispatch.
#   - Translate NEUTRAL aoided events → Melete job dispatches.
#   - NEVER treat forwarded notification text as a command — it is untrusted
#     DATA wrapped in a structured event. This is the hard security boundary.
#
# Subscription model (default-deny per class):
#   Aoided enforces default-deny subscriptions. This adapter opts into the
#   minimal set: rebuild-proposed, rice-preview-ready, notification-action.
#   It deliberately does NOT subscribe to raw notification text events — only
#   to structured, aoided-sanitised action payloads where aoided has already
#   stripped the app-title/body from the dispatch path.
#
# Integration seam:
#   The adapter is a daemon that reads aoided's event socket and emits Melete
#   job requests via the Melete API (local unix socket or HTTP, depending on
#   which Melete transport is active on this host). Skeleton: the service body
#   references `aoide` for its event-stream client; Melete's own transport
#   wiring is out of scope here.
#
# This module is in nucleus/ (not dendrites/) because per-agent adapters are
# core plumbing — they are the translation layer between the neutral bus and
# agent-specific protocols. A host disables a specific adapter by setting the
# environment variable AOIDE_ADAPTER_MELETE_ENABLE=0 or by masking the service;
# it does not need a separate dendrite flag for what is essentially a bus
# participant.
{ config, lib, pkgs, ... }:

lib.mkIf config.aoide.enable {

  systemd.user.services.aoide-melete-adapter = {
    description = "Aoide → Melete adapter — neutral events to Melete job dispatches";

    wantedBy = [ "aoided.service" ];
    after    = [ "aoided.service" ];
    bindsTo  = [ "aoided.service" ];

    serviceConfig = {
      # Skeleton: `aoide adapter melete --run` is the planned sub-command.
      # Agent B supplies the implementation; this service wire-connects it so
      # the module tree evaluates cleanly and the service is present for
      # integration testing as soon as the binary lands.
      ExecStart = "${pkgs.aoide}/bin/aoide adapter melete --run";

      Restart    = "on-failure";
      RestartSec = "5s";

      Environment = [
        # Audit log — this adapter writes its translations here, not to a
        # separate file. Single audit log, both doors (concepts/Governance).
        "AOIDE_AUDIT_LOG=${config.aoide.auditLog}"
        # Subscription manifest: only these event classes reach the adapter.
        # Values are comma-separated event class names; aoided enforces the
        # allow-list, the adapter never sees classes it has not subscribed to.
        "AOIDE_ADAPTER_SUBSCRIBE=rebuild-proposed,rice-preview-ready,notification-action"
        # Security: forwarded notification body is NEVER in the subscription
        # list. The adapter receives structured action records only.
        # (This env var is documentation-as-configuration at skeleton stage;
        # aoided reads it to build the per-adapter subscription gate.)
        "AOIDE_ADAPTER_MELETE_ENABLE=1"
        "AOIDE_USER=${config.aoide.user}"
      ];

      NoNewPrivileges = true;
      StandardOutput  = "journal";
      StandardError   = "journal";
    };

    # ── Structured event → Melete dispatch mapping (skeleton) ───────────────
    # Documented here; implemented in pkgs/aoide/src/bin/aoide.rs (Agent B).
    #
    # | aoided event class    | Melete action                               |
    # |-----------------------|---------------------------------------------|
    # | rebuild-proposed      | Post a Melete job: "user admitted rebuild"   |
    # | rice-preview-ready    | Notify Melete: preview is live, await adopt  |
    # | notification-action   | Forward structured action to Melete task     |
    #
    # Security invariant (hard rule):
    #   notification-action payloads carry { actionId, appName } — never the
    #   raw notification body or title. aoided strips the body before emitting
    #   this class. The adapter maps actionId → Melete intent; it NEVER passes
    #   app-sourced strings as Melete job prompts or instructions.
  };
}
