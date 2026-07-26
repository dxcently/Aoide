# modules/dendrites/mneme.nix — the Mneme vault MCP server.
#
# Dendrite shape v0 (CONTRACTS.md §2):
#   - Guarded on aoide.mneme.enable (default false — shipped but off).
#   - Carries its own dependencies; reads only its own aoide.mneme.* options
#     plus config.aoide.user.
#   - Enable with one line in hosts/: aoide.mneme.enable = true;
#
# What this dendrite does (ported from dxflake modules/dendrites/mneme.nix):
#   - Seeds ~/.local/bin/mneme from the pinned pkgs/mneme launcher on pin change
#     (dxflake deployed this binary purely out-of-band; Aoide adds a seed for
#     parity with the melete dendrite, still leaving the running binary free to
#     be replaced out-of-band).
#   - Runs the Mneme MCP server as a systemd service over the vault directory,
#     guarded by ConditionPathExists so it never crash-loops pre-deployment.
#   - Runs a SECOND, userspace Tailscale node dedicated to Mneme so it gets its
#     OWN :443 funnel (claude.ai connectors only reach standard :443, and the
#     host's primary node may already spend its :443 on Melete). Unprivileged,
#     isolated socket/statedir; never touches the primary tailscaled.
#
# Secret/path adaptation (Aoide has no sops stack — house rule):
#   dxflake pinned the vault at ~/Magi and the env at ~/.config/melete/mneme.env.
#   Aoide makes both runtime-configurable options (aoide.mneme.vaultPath /
#   envFile) with sane defaults; the env file is user-managed (not in the store).
#   No sops stack is introduced.
{
  lib,
  config,
  pkgs,
  ...
}:
let
  cfg = config.aoide.mneme;
  user = config.aoide.user;
  home = "/home/${user}";
in
{
  options.aoide.mneme = {
    enable = lib.mkEnableOption "the Mneme vault MCP server";

    binPath = lib.mkOption {
      type = lib.types.str;
      default = "${home}/.local/bin/mneme";
      defaultText = lib.literalExpression ''"/home/''${config.aoide.user}/.local/bin/mneme"'';
      description = "Where the runtime Mneme binary lives (seeded from pkgs/mneme; may be replaced out-of-band).";
    };

    vaultPath = lib.mkOption {
      type = lib.types.str;
      default = "${home}/Aoide-Wiki";
      defaultText = lib.literalExpression ''"/home/''${config.aoide.user}/Aoide-Wiki"'';
      description = ''
        The vault directory Mneme serves. Defaults to the Aoide wiki vault
        (dxflake used ~/Magi); the WorkingDirectory of the server.
      '';
    };

    envFile = lib.mkOption {
      type = lib.types.str;
      default = "${home}/.config/melete/mneme.env";
      defaultText = lib.literalExpression ''"/home/''${config.aoide.user}/.config/melete/mneme.env"'';
      description = ''
        EnvironmentFile for Mneme (e.g. MNEME_PUBLIC_URL, tokens), user-managed
        and not in the Nix store — the Aoide replacement for dxflake's sops
        secrets. Keep it 0600.
      '';
    };
  };

  config = lib.mkIf cfg.enable {
    # Seed the runtime binary from the pinned launcher on pin change (parity
    # with the melete dendrite; the running binary stays replaceable).
    system.activationScripts.mnemeSeed = {
      deps = [ "users" ];
      text = ''
        bindir="$(dirname ${lib.escapeShellArg cfg.binPath})"
        bin=${lib.escapeShellArg cfg.binPath}
        stamp="$bindir/.mneme-pinned"
        want="${pkgs.mneme.version}"
        if [ "$(cat "$stamp" 2>/dev/null)" != "$want" ] || [ ! -e "$bin" ]; then
          install -Dm755 ${pkgs.mneme}/bin/mneme "$bin"
          printf '%s' "$want" > "$stamp"
          chown -R ${user}:users "$bindir"
        fi
      '';
    };

    # Let the aoide user restart mneme.service / tailscaled-mneme.service
    # without auth (config/env changes applied without root). Mirrors melete.
    security.polkit.extraConfig = ''
      polkit.addRule(function(action, subject) {
        if (action.id === "org.freedesktop.systemd1.manage-units" &&
            (action.lookup("unit") === "mneme.service" ||
             action.lookup("unit") === "tailscaled-mneme.service") &&
            subject.user === "${user}") {
          return polkit.Result.YES;
        }
      });
    '';

    # Second, userspace Tailscale node dedicated to Mneme — its own :443 funnel.
    # Runs unprivileged in userspace-networking mode with an isolated
    # socket/statedir, so it never touches the primary tailscaled. Auth + funnel
    # config persist in the statedir, so it re-serves :443 -> mneme on restart
    # with no re-login.
    systemd.services.tailscaled-mneme = {
      description = "Tailscale node for Mneme (userspace, own :443 funnel)";
      after = [ "network-online.target" ];
      wants = [ "network-online.target" ];
      wantedBy = [ "multi-user.target" ];
      serviceConfig = {
        Type = "simple";
        User = user;
        Environment = [ "HOME=${home}" ];
        ExecStart = lib.concatStringsSep " " [
          "${pkgs.tailscale}/bin/tailscaled"
          "--tun=userspace-networking"
          "--socket=${home}/.local/state/tailscaled-mneme/tailscaled.sock"
          "--statedir=${home}/.local/state/tailscaled-mneme"
          "--port=0"
        ];
        Restart = "on-failure";
        RestartSec = 5;
      };
    };

    # The MCP server. ConditionPathExists guards against crash-looping until
    # binary + vault + env all exist (clean cold-host state when enabled without
    # deployment).
    systemd.services.mneme = {
      description = "Mneme — vault MCP server";
      after = [ "network-online.target" ];
      wants = [ "network-online.target" ];
      wantedBy = [ "multi-user.target" ];
      unitConfig.ConditionPathExists = [
        cfg.binPath
        cfg.vaultPath
        cfg.envFile
      ];
      serviceConfig = {
        Type = "simple";
        User = user;
        Environment = [
          "HOME=${home}"
          "PATH=${home}/.local/bin:/etc/profiles/per-user/${user}/bin:/run/current-system/sw/bin"
        ];
        EnvironmentFile = cfg.envFile;
        WorkingDirectory = cfg.vaultPath;
        ExecStart = cfg.binPath;
        Restart = "on-failure";
        RestartSec = 10;
      };
    };
  };
}
