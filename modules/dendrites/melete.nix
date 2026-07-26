# modules/dendrites/melete.nix — the Melete AI harness service.
#
# Dendrite shape v0 (CONTRACTS.md §2):
#   - Guarded on aoide.melete.enable (default false — shipped but off).
#   - Carries its own dependencies; reads only its own aoide.melete.* options
#     plus config.aoide.user / config.aoide.enable.
#   - Enable with one line in hosts/: aoide.melete.enable = true;
#
# What this dendrite does (ported from dxflake modules/dendrites/melete.nix):
#   - Seeds ~/.local/bin/melete from the pinned pkgs/melete launcher ONLY when
#     the pin changes (stamp file), leaving Melete's self-update to own the
#     running binary between bumps.
#   - Puts ~/.local/bin on PATH for non-login (Hyprland) shells too.
#   - Grants the aoide user polkit rights to restart melete.service (self-update
#     swap restarts the unit).
#   - Runs the Melete harness as a systemd service, guarded by
#     ConditionPathExists so it never crash-loops before deployment.
#
# Composition with modules/nucleus/melete-adapter.nix (NOT a duplicate):
#   The nucleus adapter (aoide-melete-adapter.service) is the neutral-bus →
#   Melete translation layer; it always runs when aoide.enable is set and only
#   DISPATCHES jobs to Melete. THIS dendrite runs Melete ITSELF (the harness the
#   adapter dispatches to). Different units, different jobs: the adapter is the
#   door, this is the room. Enabling the dendrite gives the adapter something to
#   talk to; leaving it off leaves the adapter dispatching into the void (clean).
#
# Secret/path adaptation (Aoide has no sops stack — CONTRACTS.md, house rule):
#   dxflake fetched the binary via an authenticated FOD and rendered the read
#   token into nix.conf's impure-env with sops. Aoide lifts that to a RUNTIME
#   seam: the binary is deployed out-of-band and the read token lives in a
#   runtime-configurable env file (aoide.melete.readTokenFile, default under
#   ~/.config/melete/). No secret is baked; no sops stack is introduced.
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.aoide.melete;
  user = config.aoide.user;
  home = "/home/${user}";
in
{
  options.aoide.melete = {
    enable = lib.mkEnableOption "the Melete AI harness service";

    # ── Runtime seams (replace dxflake's sops-rendered build-time secret) ────
    binPath = lib.mkOption {
      type = lib.types.str;
      default = "${home}/.local/bin/melete";
      defaultText = lib.literalExpression ''"/home/''${config.aoide.user}/.local/bin/melete"'';
      description = ''
        Where the runtime (self-updating) Melete binary lives. Seeded from the
        pinned pkgs/melete launcher on pin change; owned by Melete's self-update
        thereafter. $MELETE_BIN in the launcher wrapper reads the same default.
      '';
    };

    configFile = lib.mkOption {
      type = lib.types.str;
      default = "${home}/.config/melete/config.toml";
      defaultText = lib.literalExpression ''"/home/''${config.aoide.user}/.config/melete/config.toml"'';
      description = "Melete config TOML, deployed out-of-band (user-managed, not in the Nix store).";
    };

    envFile = lib.mkOption {
      type = lib.types.str;
      default = "${home}/.config/melete/melete.env";
      defaultText = lib.literalExpression ''"/home/''${config.aoide.user}/.config/melete/melete.env"'';
      description = ''
        EnvironmentFile for the harness (tokens/keys). Optional at service level
        (prefixed '-'), so a missing file does not fail the unit. This is the
        Aoide replacement for dxflake's sops-rendered secrets — a plain,
        user-managed runtime file. Keep it 0600.
      '';
    };

    workingDirectory = lib.mkOption {
      type = lib.types.str;
      default = "${home}/Melete";
      defaultText = lib.literalExpression ''"/home/''${config.aoide.user}/Melete"'';
      description = "Working directory for the harness (its state/checkout root).";
    };
  };

  config = lib.mkIf cfg.enable {
    # ── Reproducible baseline; self-update floats above it ───────────────────
    # Seed cfg.binPath from the pinned launcher ONLY when the pin changes
    # (stamp file). Between bumps the running binary is untouched, so Melete's
    # self-update owns it and survives reboots.
    system.activationScripts.meleteSeed = {
      deps = [ "users" ];
      text = ''
        bindir="$(dirname ${lib.escapeShellArg cfg.binPath})"
        bin=${lib.escapeShellArg cfg.binPath}
        stamp="$bindir/.melete-pinned"
        want="${pkgs.melete.version}"
        if [ "$(cat "$stamp" 2>/dev/null)" != "$want" ] || [ ! -e "$bin" ]; then
          install -Dm755 ${pkgs.melete}/bin/melete "$bin"
          printf '%s' "$want" > "$stamp"
          chown -R ${user}:users "$bindir"
        fi
      '';
    };

    # ~/.local/bin is only on login shells' PATH via ~/.profile; add it to
    # bashrc so Hyprland terminal emulators (non-login) pick it up too.
    home-manager.users.${user}.programs.bash.bashrcExtra = ''
      export PATH="$HOME/.local/bin:$PATH"
    '';

    # Let the aoide user restart melete.service without auth — the self-update
    # swap renames the new binary into place, then restarts the unit.
    security.polkit.extraConfig = ''
      polkit.addRule(function(action, subject) {
        if (action.id === "org.freedesktop.systemd1.manage-units" &&
            action.lookup("unit") === "melete.service" &&
            subject.user === "${user}") {
          return polkit.Result.YES;
        }
      });
    '';

    # The harness. ConditionPathExists guards against crash-looping on a host
    # where the binary/config have not been deployed yet (clean failure — the
    # unit simply stays inactive until deployment, which is the documented
    # cold-host state when enabled without secrets/network).
    systemd.services.melete = {
      description = "Melete — Mneme's companion AI harness";
      after = [ "network-online.target" ];
      wants = [ "network-online.target" ];
      wantedBy = [ "multi-user.target" ];
      unitConfig.ConditionPathExists = [
        cfg.binPath
        cfg.configFile
      ];
      serviceConfig = {
        Type = "simple";
        User = user;
        Environment = [
          "HOME=${home}"
          "PATH=${home}/.local/bin:${home}/.cargo/bin:/etc/profiles/per-user/${user}/bin:/run/current-system/sw/bin"
        ];
        EnvironmentFile = "-${cfg.envFile}";
        WorkingDirectory = cfg.workingDirectory;
        ExecStart = "${cfg.binPath} --config ${cfg.configFile} serve";
        Restart = "on-failure";
        RestartSec = 10;
        CPUWeight = 200;
        OOMScoreAdjust = -500;
        TimeoutStopSec = "16min";
      };
    };
  };
}
