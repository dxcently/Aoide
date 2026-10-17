# modules/dendrites/lyra/default.nix — the lyra catalogue entry: the RICE
# RENDERER (README.md). It writes the song's Hyprland rice to the state link
# the compositor sources, and nothing else. The vibeos runtime is NOT here: it
# comes only through the component providers, each importing `runtime.nix` and
# `runtime-home.nix` by path (AGENTS.md).
#
# Render order (AGENTS.md): the song's `hyprland.conf` FIRST, then
# `toHyprconf settings` LAST. The state link `~/.local/state/lyra/rice.conf`
# points at the declared twin `<root>/song/declared/rice.conf` (this file's own
# output); a stage may repoint it at a staged file without Nix.
{
  habit.home =
    {
      config,
      lib,
      pkgs,
      osConfig,
      ...
    }:
    let
      cfg = config.rice.hyprland;
      jsonFormat = pkgs.formats.json { };
      hypr = config.wayland.windowManager.hyprland;
      twin = "${osConfig.aoide.root}/song/declared/rice.conf";
      importantPrefixes = lib.unique (
        hypr.importantPrefixes ++ lib.optional hypr.sourceFirst "source" ++ [ "unbind" ]
      );
      render =
        name: attrs: raw:
        pkgs.writeText name (
          lib.optionalString (raw != null) (builtins.readFile raw + "\n")
          + lib.hm.generators.toHyprconf { inherit attrs importantPrefixes; }
        );
      # Reload only when the twin's target changed; a staged link never reaches
      # this, because the caller reloads only when the link now names the twin.
      reloadRice = ''
        if [[ "$oldTwin" != ${cfg.file} ]]; then
          (
            XDG_RUNTIME_DIR=''${XDG_RUNTIME_DIR:-/run/user/$(id -u)}
            if [[ -d "/tmp/hypr" || -d "$XDG_RUNTIME_DIR/hypr" ]]; then
              for i in $(${hypr.finalPackage}/bin/hyprctl instances -j | jq ".[].instance" -r); do
                run ${hypr.finalPackage}/bin/hyprctl -i "$i" reload config-only || true
              done
            fi
          )
        fi
      '';
    in
    {
      options.rice.hyprland = {
        settings = lib.mkOption {
          inherit (jsonFormat) type;
          default = { };
        };
        conf = lib.mkOption {
          type = lib.types.nullOr lib.types.path;
          default = null;
        };
        host = lib.mkOption {
          inherit (jsonFormat) type;
          default = { };
        };
        hostConf = lib.mkOption {
          type = lib.types.nullOr lib.types.path;
          default = null;
        };
        file = lib.mkOption {
          type = lib.types.either lib.types.package lib.types.str;
          readOnly = true;
        };
        hostFile = lib.mkOption {
          type = lib.types.nullOr (lib.types.either lib.types.package lib.types.str);
          readOnly = true;
        };
      };

      config = {
        rice.hyprland.file = render "rice-hyprland.conf" cfg.settings cfg.conf;

        rice.hyprland.hostFile =
          if cfg.host != { } || cfg.hostConf != null then
            render "rice-host-hyprland.conf" cfg.host cfg.hostConf
          else
            null;

        wayland.windowManager.hyprland.extraConfig = lib.mkIf hypr.enable (
          lib.mkAfter (
            "source = ~/.local/state/lyra/rice.conf"
            + lib.optionalString (cfg.hostFile != null) "\nsource = ${cfg.hostFile}"
          )
        );

        assertions = lib.optional hypr.enable {
          assertion = hypr.configType == "hyprlang";
          message = "lyra: wayland.windowManager.hyprland.configType is \"${hypr.configType}\", but lyra's rice lines are hyprlang sourced from hyprland.conf; set wayland.windowManager.hyprland.configType = \"hyprlang\"";
        };

        home.activation = lib.mkIf hypr.enable {
          lyraRice = lib.hm.dag.entryBetween [ "onFilesChange" ] [ "linkGeneration" ] ''
            rice="$HOME/.local/state/lyra/rice.conf"
            twin="${twin}"
            oldTwin=$(readlink "$twin" 2>/dev/null || true)
            run mkdir -p "$HOME/.local/state/lyra" "$(dirname "$twin")"
            run ln -sfn ${cfg.file} "$twin"
            if [[ ! -e "$rice" && ! -L "$rice" ]] || [[ $(readlink "$rice" 2>/dev/null || true) == "$twin" ]]; then
              run ln -sfn "$twin" "$rice"
              ${lib.optionalString (hypr.finalPackage != null) reloadRice}
            fi
          '';
        };
      };
    };
}
