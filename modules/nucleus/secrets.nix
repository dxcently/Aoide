# modules/nucleus/secrets.nix — the secrets broker's own uid + system service.
#
# Workstream SECRETS, P-V4 (deployment; ~/.claude/plans/functional-singing-boole.md
# "Workstream VAULT" section — still titled that in the plan's own
# historical text; renamed to "secrets" everywhere live at P-V4b). Provisions
# the target topology the secrets design settled on: the broker runs as ITS
# OWN system user `aoide-secrets`
# (never the operator's uid, never root), secrets home `/var/lib/aoide-secrets`
# is `0700` secrets-uid, and the socket `/run/aoide-secrets/secrets.sock` is the
# ONLY door — `0660`, group `aoide-secrets-access`, whose membership is this
# option's `aoide.secrets.members` list.
#
# This is a DEPLOYMENT module — the nix boundary (root AGENTS.md's HARD
# CONSTRAINT) explicitly carves out systemd packaging for core services, so
# a nix-dependent unit here is fine even though the `aoide secrets serve`
# BINARY itself stays nix-independent (plain Rust + unix socket + shell-outs,
# `pkgs/aoide/crates/secrets/src/home.rs`'s module doc — this file is what
# actually provisions the path that doc calls "not yet the deployed
# reality"). The non-nix install path (any other init, or none) is documented
# in `pkgs/aoide/crates/secrets/README.md`'s "Deployment" section instead —
# this module is one packaging of that same binary, not a requirement of it.
#
# Off by default, gated on `aoide.secrets.enable` (nucleus itself carries no
# `mkIf` guard on the whole file — same posture as the MCP façade/A2A door/
# usage poller in `aoided.nix`, each gated on its own flag inside an
# always-discovered file).
#
# ── Division of labor: mode vs ownership ───────────────────────────────────
# The broker (`broker::bind_socket`, P-V4 code change) chmods the socket file
# to `0660` itself, immediately after bind — that is the MODE half, and it
# holds regardless of deployment (a scratch `AOIDE_SECRETS_HOME` run by hand
# gets the same `0660`). Making that mode meaningful — giving the socket the
# RIGHT gid — is this module's job: `Group=aoide-secrets-access` below is the
# service's effective group, so every file the broker process creates
# (the socket included) is born with that gid. Neither half alone is
# sufficient: mode without the right group is just "some group can connect,
# whichever one happens to own the process"; the right group without `0660`
# mode is moot (bind's inherited umask could still leave it world-writable
# or owner-only). The code sets bits; deployment sets identity.
#
# ── Headless service-anchoring lesson (aoided.nix's own history) ───────────
# aoided.nix once anchored a USER unit to `graphical-session.target` with
# `Type=simple` on a binary that exited almost immediately — on a headless
# box with no graphical session, the unit's `wantedBy` target never fired at
# all, and even where it did, a fast clean exit flipped a `BindsTo`d door
# down with it (see that file's own comments). Two lessons land here on
# purpose: this is a SYSTEM service (own uid, `/var/lib` home — a user unit
# cannot run as a different uid without polkit gymnastics nucleus doesn't
# have), anchored to `multi-user.target` (no graphical-session dependency —
# the secrets broker has no business caring whether a desktop session exists), and
# `aoide secrets serve` BLOCKS forever on its accept loop (`broker::serve`'s
# own doc: "a running broker never returns `Ok`"), so `Type=simple` +
# `Restart=on-failure` is correct here from day one — no oneshot detour.
{
  config,
  lib,
  pkgs,
  ...
}:

lib.mkIf (config.aoide.enable && config.aoide.secrets.enable) {

  # ── The broker's own uid + the two groups it needs ───────────────────────
  # `aoide-secrets` (the service's own group, home-dir ownership) is separate
  # from `aoide-secrets-access` (the socket's group — who may CONNECT, wired
  # in via `Group=` on the service below, which the socket inherits). Kept
  # as two groups on purpose: collapsing them would mean adding an operator
  # to `aoide.secrets.members` also handed them the broker's OWN primary
  # group, which is one bit more privilege than "may reach the socket."
  users.groups.aoide-secrets = { };
  users.groups.aoide-secrets-access.members = config.aoide.secrets.members;

  users.users.aoide-secrets = {
    isSystemUser = true;
    group = "aoide-secrets";
    description = "Aoide secrets broker — owns the secrets home, never logs in";
    home = "/var/lib/aoide-secrets";
    # StateDirectory (below) provisions and owns this path at service start,
    # mode 0700 — useradd's own home-creation would run at activation time
    # with the wrong ownership story (before the service has ever bound
    # anything), so this stays false and StateDirectory is the one thing
    # that creates the directory.
    createHome = false;
    # No `shell` override: a system user's (isSystemUser = true) default
    # shell already refuses interactive login — this account exists to own
    # files and run one unit, never to be logged into.
  };

  # ── The broker system service ─────────────────────────────────────────────
  systemd.services.aoide-secrets-serve = {
    description = "Aoide secrets broker — unix-socket JSON-lines daemon, own uid";

    wantedBy = [ "multi-user.target" ];
    after = [ "multi-user.target" ];

    serviceConfig = {
      # See the module-doc "headless service-anchoring lesson" above:
      # `aoide secrets serve` blocks forever on its accept loop, so this is
      # the correct Type from day one, no oneshot-then-revert detour.
      Type = "simple";
      Restart = "on-failure";
      RestartSec = "3s";

      User = "aoide-secrets";
      # The socket's gid, not the broker's identity gid (see module doc,
      # "Division of labor: mode vs ownership") — every file
      # `broker::serve` creates, including the socket `broker::bind_socket`
      # chmods to 0660, is born with this group.
      Group = "aoide-secrets-access";

      ExecStart = "${pkgs.aoide}/bin/aoide secrets serve";

      # `/var/lib/aoide-secrets`, created + owned by User/Group above, mode
      # 0700 — the broker's own `home::secure_dir` re-asserts 0700 on every
      # `serve` startup regardless (crates/secrets/src/home.rs's module doc),
      # so this and the code agree rather than one depending on the other.
      StateDirectory = "aoide-secrets";
      StateDirectoryMode = "0700";
      # `/run/aoide-secrets`, mode 0750: owner (aoide-secrets) rwx, group
      # (aoide-secrets-access) r-x — group members need the `x` bit to reach
      # into the directory and connect to `secrets.sock` inside it; the
      # socket file itself carries the real `0660` grant (broker-side chmod
      # + this unit's Group=, both above).
      RuntimeDirectory = "aoide-secrets";
      RuntimeDirectoryMode = "0750";

      # The env tier exists exactly for this (crates/secrets/src/home.rs's
      # module doc: "Set AOIDE_SECRETS_SOCKET/AOIDE_SECRETS_HOME ... on any host
      # that hasn't run P-V4's module yet") — set explicitly rather than
      # relying on the code's own placeholder default, even though the home
      # path happens to match it today: a unit that names its own paths
      # keeps working if the crate's placeholder default ever changes.
      Environment = [
        "AOIDE_SECRETS_HOME=/var/lib/aoide-secrets"
        "AOIDE_SECRETS_SOCKET=/run/aoide-secrets/secrets.sock"
      ];

      # Modest hardening, mirroring aoided.nix's own idiom rather than a new
      # style: NoNewPrivileges is the same blanket bit every other nucleus
      # service sets. ProtectSystem=strict makes the rest of the filesystem
      # read-only to this unit (systemd auto-grants read-write back to
      # StateDirectory/RuntimeDirectory above, so the secrets home and socket
      # dir are unaffected) — justified because the broker's only writes are
      # those two paths and its backend shell-outs (pass/gopass/bw/sops,
      # crates/secrets/README.md's "Backend presets") reach their own stores
      # via PATH, not via files this unit owns. ProtectHome=true: the broker
      # has no business reading any operator's home directory, only its own
      # secrets home under /var/lib.
      NoNewPrivileges = true;
      ProtectSystem = "strict";
      ProtectHome = true;

      StandardOutput = "journal";
      StandardError = "journal";
    };
  };

  # ── Admin verbs: no sudo rule shipped, invocation documented instead ─────
  # `secrets add|rm|grant|revoke|enroll` mutate policy.json/totp.secret under
  # the secrets home, so they must run AS the secrets user — this module
  # deliberately does NOT add a `security.sudo.extraRules` entry for that
  # (no standing sudo grant to broaden). The operator invocation:
  #
  #   sudo -u aoide-secrets aoide secrets enroll
  #   sudo -u aoide-secrets aoide secrets add <name> --backend <b> --key <k>
  #   sudo -u aoide-secrets aoide secrets grant <name> <consumer>
  #
  # AOIDE_SECRETS_HOME/AOIDE_SECRETS_SOCKET are not needed on these invocations
  # when they match this module's paths — `sudo -u aoide-secrets` alone is not
  # enough to pick up `home::secrets_home`'s env override (sudo does not carry
  # the CALLER's env by default), so the CODE default
  # (`/var/lib/aoide-secrets`, `home.rs`) already matching this unit's explicit
  # env above is what makes the bare form work with no extra flags.
}
