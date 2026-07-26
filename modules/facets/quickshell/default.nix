# modules/facets/quickshell/default.nix — Quickshell shell-surface facet.
#
# Renders the complete Aoide shell surface using Quickshell (QML runtime).
# Surfaces owned: bar, notifications, launcher, osd, lockscreen, greeter,
# wallpaper, agentWidgets.
#
# Reading discipline (CONTRACTS.md §1):
#   - Reads ONLY aoide.tokens (palette + component tiers) and aoide.surfaces.
#   - Component-tier fallback (null → palette) applied locally, never pushed
#     back into the option system.
#   - NEVER reads song/ runtime paths at build time (checks.no-song-read
#     enforces this structurally).
#
# Communication discipline (entities/Quickshell):
#   - QML reads state files from song/stage/ at runtime (hot-reload).
#   - QML issues commands via the shellbridge unix socket.
#   - QML never speaks MCP or any agent protocol.
{ config, lib, pkgs, inputs, ... }:
let
  cfg = config.aoide.facets.quickshell;
  t = config.aoide.tokens;

  # ── Component-tier fallback helpers ────────────────────────────────────────
  # Each is: use the component override when set, else fall back to the palette.
  # Facets apply the fallback here (CONTRACTS.md §1 rule: "Facets apply the
  # fallback, not the option system").
  barBg     = if t.bar.bg     != null then t.bar.bg     else t.palette.bg;
  barFg     = if t.bar.fg     != null then t.bar.fg     else t.palette.fg;
  barAccent = if t.bar.accent != null then t.bar.accent else t.palette.accent;

  notifBg     = if t.notif.bg     != null then t.notif.bg     else t.palette.bg;
  notifFg     = if t.notif.fg     != null then t.notif.fg     else t.palette.fg;
  notifUrgent = if t.notif.urgent != null then t.notif.urgent else t.palette.urgent;

  # ── QML root — the full skeleton config installed into ~/Aoide/qml/ ────────
  # Each surface widget is a stub that reads its colors from tokens. The config
  # directory is placed in the user's Aoide tree so Quickshell picks it up at
  # session start. At runtime Quickshell hot-reloads from song/stage/tokens.json
  # via a FileView; the build only installs the structural QML, not the token
  # values themselves.
  quickshellConfig = pkgs.runCommand "aoide-quickshell-config" { } ''
    mkdir -p "$out/qml"
    cp -r ${./qml}/. "$out/qml/"
  '';

  # Path used at runtime: ~/Aoide/qml/shell.qml is Quickshell's entry point.
  # The user's fork provides ~/Aoide (aoide.user → /home/<user>/Aoide), so we
  # activate Quickshell pointing at that tree.
  shellQmlEntry = "/home/${config.aoide.user}/Aoide/qml/shell.qml";
in
{
  # ── Option: aoide.facets.quickshell.enable ─────────────────────────────────
  options.aoide.facets.quickshell = {
    enable = lib.mkEnableOption "Quickshell shell-surface facet";
  };

  # ── Config: only wired when the facet is enabled ───────────────────────────
  config = lib.mkIf cfg.enable {

    # ── Surface-ownership registry ──────────────────────────────────────────
    # Declare every Quickshell-owned surface. Stylix reads this registry and
    # stands down for these surfaces (concepts/Design-Tokens).
    aoide.surfaces = {
      bar.owner              = "quickshell";
      notifications.owner    = "quickshell";
      launcher.owner         = "quickshell";
      osd.owner              = "quickshell";
      lockscreen.owner       = "quickshell";
      greeter.owner          = "quickshell";
      wallpaper.owner        = "quickshell";
      agentWidgets.owner     = "quickshell";
    };

    # ── Quickshell package ──────────────────────────────────────────────────
    # The upstream Quickshell package lives in the pre-declared flake input
    # (flake.nix wires inputs.quickshell for exactly this).
    environment.systemPackages = [
      inputs.quickshell.packages.${pkgs.system}.default
    ];

    # ── Install QML config tree into the user's Aoide fork ──────────────────
    # The config tree (bar/notif/launcher/OSD/lockscreen/greeter/wallpaper)
    # lives at /home/<user>/Aoide/qml/ so it's under git control in the fork.
    # We write it via home-manager activation rather than system packages so
    # the path lands in the user's home tree.
    home-manager.users.${config.aoide.user} = {
      home.file."Aoide/qml" = {
        source = "${quickshellConfig}/qml";
        recursive = true;
      };
    };

    # ── Quickshell autostart via Hyprland exec-once ─────────────────────────
    # Placed in the Hyprland config so Quickshell starts with the compositor.
    # The compositor facet (modules/facets/compositor/) owns Hyprland's main
    # config; we add to extraConfig here — mkAfter so compositor defaults land
    # first. This uses programs.hyprland.extraConfig; if the compositor facet
    # uses a different mechanism, integration will need the autostart written
    # there instead.
    programs.hyprland.extraConfig = lib.mkAfter ''
      # Aoide Quickshell — shell surface autostart
      exec-once = quickshell -c ${shellQmlEntry}
    '';
  };
}
