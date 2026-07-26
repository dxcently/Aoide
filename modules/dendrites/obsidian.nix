# modules/dendrites/obsidian.nix — Obsidian knowledge-base integration.
#
# Dendrite shape v0 (CONTRACTS.md §2):
#   - Guarded on aoide.obsidian.enable (default false — shipped but off).
#   - Carries its own dependencies; reads no other module.
#   - Growth is additive: enable with one line in hosts/yomi-strix/default.nix.
#
# What this dendrite does:
#   - Installs Obsidian.
#   - Registers an aoided event subscription so that when a Melete job
#     completes and emits a vault-write event, shellbridge can post a
#     notification-action back through the bar widget.
#   - Writes a static shellbridge subscription fragment to
#     ~/Aoide/song/stage/obsidian-sub.json at service start, so shellbridge
#     knows which window class to watch for session jumps to vault windows.
#
# To enable on yomi-strix, add to hosts/yomi-strix/default.nix:
#   aoide.obsidian.enable = true;
{
  config,
  lib,
  pkgs,
  ...
}:
{
  options.aoide.obsidian.enable = lib.mkEnableOption "Obsidian knowledge-base integration (vault watcher + bar widget)";

  config = lib.mkIf config.aoide.obsidian.enable {

    # ── Package ───────────────────────────────────────────────────────────
    environment.systemPackages = [ pkgs.obsidian ];

    # ── Desktop entry / window-class registration ─────────────────────────
    # Obsidian uses the window class "obsidian". We wire this into shellbridge
    # so that Terminal Commander can show vault windows alongside agent windows.
    # The registration is a one-shot service that writes the fragment and exits.
    systemd.user.services.aoide-obsidian-register = {
      description = "Register Obsidian window class with shellbridge";

      wantedBy = [ "shellbridge.service" ];
      after = [ "shellbridge.service" ];

      serviceConfig = {
        Type = "oneshot";
        RemainAfterExit = true;
        # Skeleton: send a registration socket command to shellbridge.
        # Real implementation: `aoide bridge register-window-class obsidian`.
        ExecStart = "${pkgs.aoide}/bin/aoide bridge register-window-class obsidian";
        Environment = [
          "AOIDE_BRIDGE_SOCKET=%t/aoide/shellbridge.sock"
        ];
        NoNewPrivileges = true;
      };
    };

    # ── XDG MIME association ───────────────────────────────────────────────
    # Register Obsidian as the handler for obsidian:// URIs so aoided can
    # deep-link into a vault note from a structured notification-action.
    # xdg.mime.defaultApplications is the NixOS system-level MIME registry;
    # for per-user overrides the Quickshell facet or home-manager can extend.
    xdg.mime.defaultApplications = {
      "x-scheme-handler/obsidian" = "obsidian.desktop";
    };
  };
}
