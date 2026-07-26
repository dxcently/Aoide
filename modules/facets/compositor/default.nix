# modules/facets/compositor/default.nix — Hyprland compositor facet.
#
# Wires Hyprland as the NixOS Wayland compositor and applies compositor-side
# design tokens live via hyprctl. Token values (gaps, radius, borders, blur)
# are baked into the Hyprland config at build time so they take effect on
# session start; the token emitter package (pkgs/tokens) can re-dispatch
# them live via hyprctl during a rehearsal (preview) pass.
#
# IPC socket: exposes the Hyprland IPC socket path for shellbridge to consume.
# shellbridge uses it to track windows and dispatch focus commands
# (hyprctl dispatch focuswindow) backing the Terminal-Commander session-jump
# flow (concepts/Desktop-Architecture).
#
# Reading discipline (CONTRACTS.md §1):
#   - Reads ONLY aoide.tokens (palette + component tiers).
#   - Component-tier fallback applied locally.
#   - NEVER reads song/ runtime paths (checks.no-song-read enforced structurally).
{ config, lib, ... }:
let
  cfg = config.aoide.facets.compositor;
  t = config.aoide.tokens;

  # ── Component-tier fallback helpers ────────────────────────────────────────
  windowBorder         = if t.window.border         != null then t.window.border         else t.palette.accent;
  windowBorderInactive = if t.window.borderInactive  != null then t.window.borderInactive  else t.palette.bg;

  # ── Derived geometry values ───────────────────────────────────────────────
  # v0 has no geometry-tier tokens; these are opinionated defaults that serve
  # as the immutable baseline. When a geometry tier is added (v1), replace
  # these with token reads.
  gapOuter = 8;
  gapInner = 6;
  borderWidth = 2;
  rounding = 8;   # window corner radius (px)
  blurEnabled = true;
  blurPasses = 3;
  blurSize = 8;

  # ── Hyprland config fragment — tokens baked in at build time ─────────────
  # The token emitter (pkgs/tokens, Agent A) re-runs hyprctl keyword dispatch
  # during rehearsal to live-patch these values without a rebuild.
  hyprTokenConfig = ''
    # ── Aoide design tokens — compositor facet ───────────────────────────
    # Generated from aoide.tokens at build time; live-patched by aoide-tokens
    # emitter during rice preview (hyprctl keyword).

    general {
        gaps_out = ${toString gapOuter}
        gaps_in  = ${toString gapInner}
        border_size = ${toString borderWidth}
        col.active_border   = rgb(${lib.removePrefix "#" windowBorder})
        col.inactive_border = rgb(${lib.removePrefix "#" windowBorderInactive})
    }

    decoration {
        rounding = ${toString rounding}

        blur {
            enabled = ${if blurEnabled then "true" else "false"}
            size    = ${toString blurSize}
            passes  = ${toString blurPasses}
        }
    }
  '';

  # ── Hyprland keybinds for Aoide workflows ─────────────────────────────────
  # These are the binds the Quickshell facet depends on. Placed here so the
  # compositor facet owns all hyprctl-level wiring.
  hyprBindConfig = ''
    # ── Aoide keybinds ────────────────────────────────────────────────────
    # Launcher (shellbridge → AoideLauncher toggle)
    bind = SUPER, SPACE, exec, aoide shell launcher toggle

    # Lock screen
    bind = SUPER, L, exec, aoide shell lock

    # Rice preview / adopt shortcuts
    bind = SUPER SHIFT, P, exec, aoide rice preview
    bind = SUPER SHIFT, A, exec, aoide rice adopt
  '';
in
{
  # ── Option: aoide.facets.compositor.enable ────────────────────────────────
  options.aoide.facets.compositor = {
    enable = lib.mkEnableOption "Hyprland compositor facet";
  };

  # ── Config: only wired when the facet is enabled ──────────────────────────
  config = lib.mkIf cfg.enable {

    # ── Enable Hyprland via NixOS programs.hyprland ────────────────────────
    # programs.hyprland.enable installs Hyprland, sets up the session
    # entry, and configures the NixOS service layer. The flake input
    # (inputs.hyprland) is pre-declared in flake.nix.
    #
    # NOTE: the NixOS-level module has no extraConfig — the config FILE is
    # owned by home-manager's wayland.windowManager.hyprland below. System
    # layer = session/portal; home layer = hyprland.conf. package = null in
    # the home module so Hyprland is installed exactly once (system side).
    programs.hyprland.enable = true;

    home-manager.users.${config.aoide.user} = {
      wayland.windowManager.hyprland = {
        enable = true;
        package = null;        # system programs.hyprland provides the binary
        portalPackage = null;  # and the portal
        configType = "hyprlang";  # explicit: classic hyprland.conf, not lua
        # Bake token + keybind config fragments into hyprland.conf.
        # mkBefore so token defaults land before any per-user overrides
        # (the quickshell facet appends its autostart with mkAfter).
        extraConfig = lib.mkBefore (hyprTokenConfig + hyprBindConfig);
      };
    };

    # ── XDG portal for Hyprland ────────────────────────────────────────────
    xdg.portal = {
      enable = true;
      extraPortals = [ ];  # xdg-desktop-portal-hyprland added via programs.hyprland
    };

    # ── Environment variables for the Hyprland session ─────────────────────
    # HYPRLAND_INSTANCE_SIGNATURE and XDG_CURRENT_DESKTOP are set by Hyprland
    # itself; we expose the socket path for shellbridge.
    environment.sessionVariables = {
      # shellbridge reads this to locate the Hyprland IPC socket:
      #   $XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/.socket.sock
      # This variable is set by Hyprland at session start; we advertise the
      # pattern so shellbridge can construct the path without hardcoding it.
      AOIDE_HYPRLAND_SOCKET_PATTERN = "$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/.socket.sock";

      # Wayland-first hints for common apps
      NIXOS_OZONE_WL = "1";
      MOZ_ENABLE_WAYLAND = "1";
      QT_QPA_PLATFORM = "wayland";
      GDK_BACKEND = "wayland,x11";
      SDL_VIDEODRIVER = "wayland";
    };

    # ── greetd (login greeter) ─────────────────────────────────────────────
    # Quickshell greeter surface needs greetd as the session manager.
    # The Quickshell greeter will implement the greetd IPC protocol.
    # STUB: greetd.enable wired here; actual Quickshell greeter session
    # command is set when AoideGreeter.qml implements greetd protocol.
    services.greetd = {
      enable = true;
      settings.default_session = {
        command = "${lib.getExe' config.programs.hyprland.package "Hyprland"}";
        user = config.aoide.user;
      };
    };
  };
}
