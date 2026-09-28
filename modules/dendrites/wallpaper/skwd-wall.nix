# modules/dendrites/wallpaper/skwd-wall.nix — the external-engine provider of
# the `wallpaper` capability.
#
# It installs the engine's three packages, runs one `skwd-walld` user unit of its
# own, and keeps the engine's own config to what Aoide owns: theming off, its
# library inside `aoide.root`, one post-processing hook. WHO PAINTS is the host's
# choice, so no song is consulted — the seam is the fact
# `aoide.wallpaper.provider = "skwd-wall"` (CONTRACTS.md §0), which this file SETS
# `mkDefault` and its `body` guards on.
#
# `AOIDE_SKWD_WALL_STANDIN` is the step-aside image — `lyra cover sync`'s
# fallback when the installed release has no `clear` verb — exported to this unit
# (its `ExecStartPost` and the hook it spawns) and to the session, so one
# variable is the whole switch point.
let
  body =
    {
      config,
      lib,
      pkgs,
      aoideInputs,
      ...
    }:
    let
      # The engine's own three packages from the pre-declared flake input
      # (flake.nix): the picker, the daemon (with the `skwd-helm` client) and
      # the paper renderer. Never the suite `default`, which drags the
      # semantic-search lens and its model pack, and never the unfree
      # steamworks helper.
      skwd = aoideInputs.skwd-wall.packages.${pkgs.stdenv.hostPlatform.system};

      # Everything the engine owns, under the runtime root: its config
      # directory, its cache and the two folders its library scans. Pointing
      # `paths.*` here is what keeps it from indexing `~/Pictures/Wallpapers`
      # (the upstream default), and pointing `SKWD_WALL_V2_CONFIG` here is what
      # makes the app's in-place rewrite of `config.json` land in a host-owned
      # file instead of fighting a store symlink.
      stateDir = "${config.aoide.root}/state/skwd-wall";
      configDir = "${stateDir}/config";
      cacheDir = "${stateDir}/cache";
      staticDir = "${stateDir}/library/static";
      videoDir = "${stateDir}/library/video";

      # The one hook the engine is allowed to run: it tells Aoide what the
      # engine now shows, and RECORDS it — `--from-skwd` never applies anything
      # back to the engine, which is what makes the round trip terminate. The
      # daemon single-quotes each substituted value itself, so the placeholders
      # are spelled bare (quotes here would become part of the path).
      postProcess = "lyra cover set --from-skwd --kind %type% %path%";

      # Where a pick the engine cannot paint leaves the shell's own layer:
      # applying a fully transparent 1x1 still paints nothing, so the wallpaper
      # surface below it shows again.
      standinPng =
        pkgs.runCommand "aoide-skwd-wall-standin.png"
          {
            nativeBuildInputs = [ pkgs.imagemagick ];
          }
          ''
            convert -size 1x1 xc:none "$out"
          '';

      # The engine's config as Aoide requires it: a jq MERGE over whatever the
      # file already holds, enforcing the keys Aoide owns and keeping every
      # other setting the user made. A file the app has rewritten in place is
      # the input of the next merge, never an error.
      #
      # The keys the engine's own config catalog names, and only those: the
      # whole-fan-out theming switch off, the bundled renderer over `awww`, this
      # layer (the shell's own), the library and cache inside this lane's state
      # dir, no restore-on-startup (a restore would repaint over a song staged
      # with the shell's own layer — `ExecStartPost` re-applies instead), and the
      # one hook, replaced rather than appended.
      engineConfig = pkgs.writeShellScript "aoide-skwd-wall-config" ''
        set -euo pipefail
        mkdir -p "${configDir}" "${cacheDir}" "${staticDir}" "${videoDir}"
        cfg="${configDir}/config.json"
        [ -s "$cfg" ] || printf '{}\n' > "$cfg"
        tmp=$(mktemp "${configDir}/.config.json.XXXXXX")
        ${pkgs.jq}/bin/jq -S \
          --arg wallpaper "${staticDir}" \
          --arg video "${videoDir}" \
          --arg hook ${lib.escapeShellArg postProcess} '
            .theme = ((.theme // {}) * { policy: "off" })
            | .paper = ((.paper // {}) * { engine: "skwd-paper", wallpaperLayer: "background" })
            | .paths = ((.paths // {}) * { wallpaper: $wallpaper, videoWallpaper: $video })
            | .restoreOnStartup = false
            | .postProcessing = [{ type: "all", command: $hook }]
          ' "$cfg" > "$tmp"
        mv -f "$tmp" "$cfg"
      '';
    in
    {
      config = lib.mkIf (config.aoide.wallpaper.provider == "skwd-wall") {

        # ── The engine's packages ──────────────────────────────────────────────
        # The picker (`skwd-wall-v2`) is the user's own door onto the engine's
        # library, and `skwd-helm` (inside `skwd-deck`) is the one Aoide's
        # bridge drives — both reachable from a terminal, which is what makes
        # the pick a capability and not a QML affordance (root AGENTS.md rule 7).
        environment.systemPackages = [
          skwd.skwd-wall-v2
          skwd.skwd-deck
          skwd.skwd-paper
        ];

        # The step-aside image, in the session environment too: a terminal
        # `lyra cover sync` is the repair path when a pick is dropped by hand.
        environment.sessionVariables.AOIDE_SKWD_WALL_STANDIN = "${standinPng}";

        home-manager.users.${config.aoide.user} =
          { lib, ... }:
          {
            # The config merge runs on every activation, before the session: the
            # app rewrites this file in place, so the merge is what keeps
            # Aoide-owned keys true while the user's own settings survive.
            home.activation.aoideSeedSkwdWallConfig =
              lib.hm.dag.entryBetween [ "reloadSystemd" ] [ "writeBoundary" ]
                ''
                  run ${engineConfig}
                '';

            # ── The control daemon ───────────────────────────────────────────────
            # A user service, not the upstream NixOS module: that module installs
            # the whole 856 MiB suite and defines a second unit of the same name
            # (one that also lacks this lane's env). One unit, one producer.
            #
            # `ExecStartPost` re-asserts the staged state on every start: the
            # provider's own restore is off, so after a daemon restart (or a first
            # start) nothing would repaint until the next pick — `lyra cover
            # sync` is what puts the staged pick back in front of the user. It
            # runs `lyra` off this unit's PATH, and it waits out the daemon's own
            # socket (this hook can win that race).
            #
            # `ConditionPathExists` guards the merged config: without it a unit
            # started before the first activation would make the daemon write a
            # default config the next merge then has to correct. The Wayland
            # condition is the shell's own — the engine paints on a session.
            systemd.user.services.skwd-walld = {
              Unit = {
                Description = "skwd-wall daemon — the external wallpaper engine's control daemon";
                PartOf = [ "graphical-session.target" ];
                After = [ "graphical-session.target" ];
                ConditionEnvironment = "WAYLAND_DISPLAY";
                ConditionPathExists = "${configDir}/config.json";
                StartLimitIntervalSec = 60;
                StartLimitBurst = 5;
              };
              Service = {
                ExecStart = "${skwd.skwd-deck}/bin/skwd-walld";
                ExecStartPost = "${pkgs.aoide.rice}/bin/lyra cover sync";
                Environment = [
                  "SKWD_WALL_V2_CONFIG=${configDir}"
                  "SKWD_WALL_V2_CACHE=${cacheDir}"
                  "AOIDE_ROOT=${config.aoide.root}"
                  "AOIDE_SKWD_WALL_STANDIN=${standinPng}"
                  # PATH replaces the manager's: every bare name the daemon execs
                  # (`sh`/`setsid`, `ffmpeg`, `file`, `lyra`, the renderer).
                  "PATH=${
                    lib.makeBinPath [
                      pkgs.aoide.rice
                      skwd.skwd-deck
                      skwd.skwd-paper
                      pkgs.bash
                      pkgs.coreutils
                      pkgs.file
                      pkgs.ffmpeg
                      pkgs.util-linux
                    ]
                  }"
                ];
                Restart = "on-failure";
                RestartSec = 3;
              };
              Install.WantedBy = [ "graphical-session.target" ];
            };

            # ── Re-assert the engine after an activation ─────────────────────────
            # Only a START runs the unit's `ExecStartPost` sync, and systemd does
            # not restart a unit whose file merely changed: ask for one restart.
            home.activation.aoideResyncSkwdWall = lib.hm.dag.entryAfter [ "reloadSystemd" ] ''
              run env XDG_RUNTIME_DIR=/run/user/$(${pkgs.coreutils}/bin/id -u) \
                ${pkgs.systemd}/bin/systemctl --user try-restart skwd-walld.service || true
            '';
          };
      };
    };
in
{
  inherit body;

  # The chosen provider NAMES ITSELF: this file is only ever imported when the
  # host selected it (the registry's line, `modules/dendrites/wallpaper/
  # default.nix`), so the fact it sets is the answer to "who paints here" — and
  # the runtime reads that answer off the same document the shell does
  # (CONTRACTS.md §4). `mkDefault`, like every fact: a host that wants this
  # lane's packages but the shell's layer overrides it and this `body` stands
  # down.
  nixos =
    { lib, ... }:
    {
      imports = [ body ];
      config.aoide.wallpaper.provider = lib.mkDefault "skwd-wall";
    };
}
