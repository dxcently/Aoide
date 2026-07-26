# modules/facets/compositor/default.nix — Hyprland compositor facet.
#
# Wires Hyprland as the NixOS Wayland compositor and applies compositor-side
# notes live via hyprctl. Note values (gaps, radius, borders, blur)
# are baked into the Hyprland config at build time so they take effect on
# session start; the note emitter package (pkgs/notes) can re-dispatch
# them live via hyprctl during a rehearsal (preview) pass.
#
# IPC socket: exposes the Hyprland IPC socket path for shellbridge to consume.
# shellbridge uses it to track windows and dispatch focus commands
# (hyprctl dispatch focuswindow) backing the Terminal-Commander session-jump
# flow (concepts/Desktop-Architecture).
#
# Reading discipline (CONTRACTS.md §1):
#   - Reads ONLY aoide.notes (palette + component tiers).
#   - Component-tier fallback applied locally.
#   - NEVER reads song/ runtime paths (checks.no-song-read enforced structurally).
{ config, lib, ... }:
let
  cfg = config.aoide.facets.compositor;
  t = config.aoide.notes;

  # ── Component-tier fallback helpers ────────────────────────────────────────
  windowBorder = if t.window.border != null then t.window.border else t.palette.accent;
  windowBorderInactive =
    if t.window.borderInactive != null then t.window.borderInactive else t.palette.bg;

  # ── Derived geometry values ───────────────────────────────────────────────
  # v0 has no geometry-tier notes; these are opinionated defaults that serve
  # as the immutable baseline. When a geometry tier is added (v1), replace
  # these with note reads.
  gapOuter = 8;
  gapInner = 6;
  borderWidth = 2;
  rounding = 8; # window corner radius (px)
  blurEnabled = true;
  blurPasses = 3;
  blurSize = 8;

  # ── Hyprland config fragment — notes baked in at build time ──────────────
  # The note emitter (pkgs/notes, Agent A) re-runs hyprctl keyword dispatch
  # during rehearsal to live-patch these values without a rebuild.
  hyprNoteConfig = ''
    # ── Aoide notes — compositor facet ───────────────────────────────────
    # Generated from aoide.notes at build time; live-patched by aoide-notes
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

    # Gadget dock popup (shellbridge → AoideAgentWidgets open-and-pin).
    # SUPER+G summons the LEFT-edge pinnable dock popup, which CONTAINS the DAG
    # gadget (the dock is now the primary DAG affordance). The dock also opens
    # on mouse hot-edge hover (pure QML). The standalone AoideSessionGraph
    # overlay keeps NO bind — it is bridge-only/dormant (see its header note).
    bind = SUPER, G, exec, aoide shell dock toggle

    # Rice preview / adopt shortcuts
    bind = SUPER SHIFT, P, exec, aoide rice preview
    bind = SUPER SHIFT, A, exec, aoide rice adopt

    # ── Window management (ported from dxflake hyprland dendrite) ────────
    # Normalized to SUPER, matching the Aoide binds above. dxflake exec
    # binds for tools Aoide doesn't ship (rofi, thunar, cliphist,
    # hyprshot/satty, vesktop/discord, gpu-screen-recorder) are dropped —
    # the launcher and the bar's power cell cover those seams. dxflake's
    # media/brightness XF86 keys are also deliberately NOT bound (strict
    # window-management scope; the bar's volume cell owns audio by mouse) —
    # a known seam if hardware keys are wanted later.

    # Terminal (the kitty dendrite ships kitty)
    bind = SUPER, RETURN, exec, kitty

    # Window controls
    bind = SUPER, Q, killactive
    bind = SUPER, V, togglefloating
    bind = SUPER, F, fullscreen

    # Focus movement — arrows carry the full left/down/up/right set; H/J/K
    # add vim left/down/up. dxflake's SUPER+L (focus right) is NOT ported:
    # SUPER+L is the Aoide lock bind above, so right stays arrow-only.
    bind = SUPER, left, movefocus, l
    bind = SUPER, down, movefocus, d
    bind = SUPER, up, movefocus, u
    bind = SUPER, right, movefocus, r
    bind = SUPER, H, movefocus, l
    bind = SUPER, J, movefocus, d
    bind = SUPER, K, movefocus, u

    # Move window (same directional scheme)
    bind = SUPER SHIFT, left, movewindow, l
    bind = SUPER SHIFT, down, movewindow, d
    bind = SUPER SHIFT, up, movewindow, u
    bind = SUPER SHIFT, right, movewindow, r
    bind = SUPER SHIFT, H, movewindow, l
    bind = SUPER SHIFT, J, movewindow, d
    bind = SUPER SHIFT, K, movewindow, u

    # Resize — binde repeats while held (dxflake's step sizes)
    binde = SUPER ALT, left, resizeactive, -20 0
    binde = SUPER ALT, down, resizeactive, 0 40
    binde = SUPER ALT, up, resizeactive, 0 -40
    binde = SUPER ALT, right, resizeactive, 20 0
    binde = SUPER ALT, H, resizeactive, -20 0
    binde = SUPER ALT, J, resizeactive, 0 40
    binde = SUPER ALT, K, resizeactive, 0 -40

    # Workspaces 1–10 — pairs with the bar's musical workspace glyphs
    bind = SUPER, 1, workspace, 1
    bind = SUPER, 2, workspace, 2
    bind = SUPER, 3, workspace, 3
    bind = SUPER, 4, workspace, 4
    bind = SUPER, 5, workspace, 5
    bind = SUPER, 6, workspace, 6
    bind = SUPER, 7, workspace, 7
    bind = SUPER, 8, workspace, 8
    bind = SUPER, 9, workspace, 9
    bind = SUPER, 0, workspace, 10
    bind = SUPER SHIFT, 1, movetoworkspace, 1
    bind = SUPER SHIFT, 2, movetoworkspace, 2
    bind = SUPER SHIFT, 3, movetoworkspace, 3
    bind = SUPER SHIFT, 4, movetoworkspace, 4
    bind = SUPER SHIFT, 5, movetoworkspace, 5
    bind = SUPER SHIFT, 6, movetoworkspace, 6
    bind = SUPER SHIFT, 7, movetoworkspace, 7
    bind = SUPER SHIFT, 8, movetoworkspace, 8
    bind = SUPER SHIFT, 9, movetoworkspace, 9
    bind = SUPER SHIFT, 0, movetoworkspace, 10
    bind = ALT, Tab, workspace, previous

    # Special workspaces — the bar's icon map carries magic and scratch
    bind = SUPER, X, togglespecialworkspace, magic
    bind = SUPER, Z, togglespecialworkspace, scratch
    bind = SUPER SHIFT, X, movetoworkspace, special:magic
    bind = SUPER SHIFT, Z, movetoworkspace, special:scratch

    # Mouse — SUPER+leftdrag move, SUPER+rightdrag resize
    bindm = SUPER, mouse:272, movewindow
    bindm = SUPER, mouse:273, resizewindow
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
        package = null; # system programs.hyprland provides the binary
        portalPackage = null; # and the portal
        configType = "hyprlang"; # explicit: classic hyprland.conf, not lua
        # Bake note + keybind config fragments into hyprland.conf.
        # mkBefore so note defaults land before any per-user overrides
        # (the quickshell facet appends its autostart with mkAfter).
        extraConfig = lib.mkBefore (hyprNoteConfig + hyprBindConfig);
      };
    };

    # ── XDG portal for Hyprland ────────────────────────────────────────────
    xdg.portal = {
      enable = true;
      extraPortals = [ ]; # xdg-desktop-portal-hyprland added via programs.hyprland
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
