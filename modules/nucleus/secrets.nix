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
  inputs,
  ...
}:

lib.mkMerge [
  (lib.mkIf (config.aoide.enable && config.aoide.secrets.enable) {

  # ── qrencode on the operator's own PATH ──────────────────────────────────
  # `secrets enroll` (run by the operator, not the broker service — see
  # "Admin commands" below) renders its `otpauth://` URI as a QR code only when
  # `qrencode` is reachable on PATH (`enroll::render_qr`'s feature-detect,
  # not a Cargo dependency); without it the URI/base32 still print, just no
  # QR. The first live enrollment attempt (yomi-strix, P-V4d) found
  # `qrencode` missing from an ordinary operator shell. Not on the broker
  # unit's own `path` above — enrollment is invoked by hand, never by the
  # service itself.
  environment.systemPackages = [
    pkgs.qrencode
    # AGE LANE judge finding #2: `secrets migrate` is a direct-home admin op
    # (sudo -u aoide-secrets, never the socket), so its age/age-keygen
    # shell-outs run under sudo's secure_path — the unit-path entry above
    # covers only the broker's own resolve/put. Without this, the lane's
    # headline command fails on every deployed box with the taught
    # missing-binary hint. Same class as the qrencode lesson at the top of
    # this list.
    pkgs.age
  ]
  # ── zenity for `secrets watch --popup` ───────────────────────────────────
  # Same shape as qrencode above: a hand-invoked command (`watch --popup`,
  # crates/secrets/src/watch.rs) feature-detects a PATH binary
  # (`zenity_available`) and prints a taught install hint when absent — not a
  # Cargo dependency, not on the broker unit's `path`. Gated on the
  # quickshell facet because the popup is a desktop surface: a headless box
  # (sakaki) enables the broker but has no display for a dialog, and
  # ungated zenity would drag GTK into its closure.
  ++ lib.optional config.aoide.facets.quickshell.enable pkgs.zenity;

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

    # P-V4d, live-found on yomi-strix: a systemd unit's default PATH carries
    # no `sh` — every backend template (`backend.rs`'s `fetch_value`/
    # `store_value`) runs via `sh -c`, so with NO path at all the broker
    # bound its socket fine, passed the policy gate, and then failed every
    # resolve/put with "spawning backend `file`: No such file or directory".
    # `bash` supplies `sh` (verified: nixpkgs' bash output carries a `sh`
    # binary alongside `bash` itself); `coreutils` covers `cat`/`mkdir`/
    # `install`, the three tools the built-in `file` backend's own
    # GET/SET templates shell out to (`backend.rs`'s `FILE_BACKEND_GET`/
    # `FILE_BACKEND_SET`). A `pass`/`gopass`/`bw`/`sops` backend template
    # reaching further than that is the operator's own PATH concern, same as
    # any other preset (README's "Backend presets").
    path = [
      pkgs.bash
      pkgs.coreutils
      # AGE LANE (P-G1): the built-in age backend's templates shell out to
      # `age`/`age-keygen` AS THE BROKER (encrypt/decrypt/lazy identity mint
      # all happen on the broker side of the socket), so the binary belongs
      # on the UNIT's path — not systemPackages; the operator's shell never
      # runs age itself. The code stays nix-independent: a missing binary
      # earns a taught install-hint error, this line is just one packaging.
      pkgs.age
    ];

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
      # relying on the code's own defaults, even though BOTH paths below now
      # match the code's own defaults exactly (`home.rs`'s placeholder and,
      # as of P-V4d, `socket.rs`'s canonical `/run` path too): a unit that
      # names its own paths keeps working if either crate default ever
      # changes.
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

  # ── Admin commands: no sudo rule shipped, invocation documented instead ─────
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
  })

  # ── The popup watcher: a graphical-session USER unit ──────────────────────
  # Nothing autostarted `aoide secrets watch --popup` before this — a parked
  # TOTP ask surfaced only if an operator happened to have a `secrets watch`
  # already running in some terminal. This unit closes that gap the same way
  # shellbridge.nix's own units do: a per-session, per-operator process
  # anchored to `graphical-session.target`, never a system service (the popup
  # is a desktop surface belonging to the logged-in operator, not the
  # secrets-uid broker).
  #
  # Gated on `config.aoide.facets.quickshell.enable` — the SAME condition
  # `environment.systemPackages`'s `pkgs.zenity` entry above already uses: a
  # headless box (sakaki) enables `aoide.secrets` for its A2A door's own
  # bearer-token resolve but has no display for a dialog, and ungated zenity
  # would drag GTK into that box's closure for nothing this unit could ever
  # show. `secrets watch --popup` itself feature-detects BOTH dialog binaries
  # at runtime (`watch::resolve_lyra_bin`/`watch::zenity_available`,
  # `crates/secrets/src/watch.rs`) and refuses to start only when neither
  # resolves — this gate exists purely to keep the unit off a box with no
  # graphical session at all, not to pick which binary it uses.
  #
  # Like the broker service above (module doc's "headless service-anchoring
  # lesson"), `aoide secrets watch --popup` blocks forever on its own tail
  # loop — `Type = simple` + `Restart = on-failure` is correct from day one,
  # no oneshot detour. No explicit ordering against `aoide-secrets-serve`
  # (a SYSTEM unit — crossing manager boundaries for ordering is unusual and
  # unnecessary here): `watch::wait_for_follower` already waits out a
  # not-yet-created events feed rather than exiting 1, so this unit coming up
  # before the broker has bound its socket is a normal, harmless race, not a
  # failure to guard against.
  (lib.mkIf (config.aoide.enable && config.aoide.secrets.enable && config.aoide.facets.quickshell.enable) {
    systemd.user.services.aoide-secrets-watch = {
      description = "Aoide secrets popup watcher — surfaces parked TOTP asks as a dialog";

      wantedBy = [ "graphical-session.target" ];
      after = [ "graphical-session.target" ];
      partOf = [ "graphical-session.target" ];

      # `zenity` must resolve off a bare-name `PATH` lookup
      # (`watch::zenity_available`/`watch::spawn_zenity_entry`, both take the
      # binary NAME, never a hardcoded path) — a systemd user unit's default
      # `PATH` does not include `/run/current-system/sw/bin`'s
      # `environment.systemPackages` entries any more reliably than
      # shellbridge.nix's own `hyprctl`/`curl` lesson already found, so this
      # is listed explicitly rather than assumed from the systemPackages
      # entry above (that entry serves `secrets enroll`'s own QR rendering
      # and an operator's interactive shell, never this unit).
      # `quickshell` rides the same rule for the lyra dialog path:
      # `lyra secrets ask` spawns it by bare name, so the unit that spawns
      # lyra must carry it — the live gap that let the first unit-spawned
      # dialog fail silently (lyra resolved via AOIDE_RICE_BIN, its
      # quickshell ENOENT'd, the ask just stayed parked). Same package the
      # quickshell facet installs; this whole block is gated on that facet.
      path = [
        pkgs.zenity
        inputs.quickshell.packages.${pkgs.stdenv.hostPlatform.system}.default
      ];

      serviceConfig = {
        Type = "simple";
        Restart = "on-failure";
        RestartSec = "3s";

        ExecStart = "${pkgs.aoide}/bin/aoide secrets watch --popup";

        Environment = [
          # Belt-and-suspenders, same reasoning as the broker service's own
          # explicit env above: matches `socket.rs`'s canonical default today,
          # keeps working if that default ever changes.
          "AOIDE_SECRETS_SOCKET=/run/aoide-secrets/secrets.sock"
        ]
        # `watch::resolve_lyra_bin` resolves `lyra` via `AOIDE_RICE_BIN` (tier
        # 1, trusted unconditionally) or a sibling of `current_exe()` (tier
        # 2) — but `lyra` ships from `pkgs.aoide.rice`, a SEPARATE output
        # from the `aoide` binary this unit execs (P-A8 of the binary-split
        # workstream, the exact sibling-resolution break shellbridge.nix's
        # own `AOIDE_CORE_BIN` comment already documents in the other
        # direction), so sibling resolution would silently fail here without
        # this. Only set when `aoide.lyra.enable` is actually on — mirrors
        # shellbridge.nix's own guard, since a host with the facet on but
        # lyra explicitly disabled has no `pkgs.aoide.rice` output to point
        # at.
        ++ lib.optional config.aoide.lyra.enable "AOIDE_RICE_BIN=${pkgs.aoide.rice}/bin/lyra";

        NoNewPrivileges = true;
        StandardOutput = "journal";
        StandardError = "journal";
      };
    };
  })
]
