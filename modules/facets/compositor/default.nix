# modules/facets/compositor/default.nix — Hyprland compositor facet.
#
# Wires Hyprland as the NixOS Wayland compositor and applies compositor-side
# notes live via hyprctl. Note values (gaps, radius, borders, blur)
# are baked into the Hyprland config at build time so they take effect on
# session start; the note emitter package (pkgs/drachma) can re-dispatch
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
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.aoide.facets.compositor;
  t = config.aoide.notes;

  # ── Component-tier fallback helpers ────────────────────────────────────────
  # No hex lives here by design (CONTRACTS.md §1/§5): the facet is host- and
  # song-agnostic, so border colour is ALWAYS a note read, never a literal.
  # This IS the substitution seam — window.border/window.borderInactive (or
  # their palette.accent/palette.bg fallback) are set per-song in that song's
  # rice.nix (e.g. song/repertoire/hero/rice.nix), which maps its own base16
  # roles onto the component tier. For the Pantheon hero song, the intended
  # mapping is base0C wireCyan (#5fd8e8) → window.border (active) and a dark
  # muted ground, base01 (#141419) or base02 (#26262e) → window.borderInactive
  # — but that assignment belongs in hero/rice.nix, not in this facet; today
  # hero/rice.nix still carries the older dxflake-era BW pair (border =
  # "#ffffff", borderInactive = "#000000"), so a song-side edit is what would
  # actually change the rendered colour.
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
  # Edged windows (khoa, with the bw border key): square corners — the
  # dxflake read. The Pantheon wireframe language wants hard outlines too.
  rounding = 0; # window corner radius (px)
  blurEnabled = true;
  blurPasses = 3;
  blurSize = 8;

  # ── Hyprland config fragment — notes baked in at build time ──────────────
  # The note emitter (pkgs/drachma, Agent A) re-runs hyprctl keyword dispatch
  # during rehearsal to live-patch these values without a rebuild.
  hyprNoteConfig = ''
    # ── Aoide notes — compositor facet ───────────────────────────────────
    # Generated from aoide.notes at build time; live-patched by the drachma
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

    # Glass for the quickshell surfaces (dxflake's "namespace waybar" rule,
    # aoide-native namespaces — set per-PanelWindow in shell.qml). Blur reads
    # through the bar's translucent barBg and the dock's Aero frames; the
    # wallpaper surface (aoide-wallpaper) is deliberately NOT blurred.
    # ignore_alpha keeps the surfaces' fully-transparent regions (the dock's
    # retracted drawer, the bar's popout gutter) from rendering as a grey
    # blur stripe. (Field names per the 0.5x rules rework: "ignorealpha" is
    # rejected by hyprctl on 0.56 — verified live.)
    layerrule = blur on, match:namespace aoide-bar
    layerrule = blur on, match:namespace aoide-dock
    layerrule = ignore_alpha 0.05, match:namespace aoide-bar
    layerrule = ignore_alpha 0.05, match:namespace aoide-dock
    # blur_popups extends the glass to the bar's PopupWindow children (the
    # gadget popouts) — same 0.5x snake_case rework spelling as ignore_alpha.
    layerrule = blur_popups on, match:namespace aoide-bar

    # hyprglass (pkgs/hyprglass, loaded via the HM plugins list below):
    # Liquid Glass on the quickshell surfaces, ON TOP of the blur+gloss —
    # refraction/fresnel the flat gradient can't fake. Same namespaces as
    # the layerrules; the wallpaper surface stays untouched.
    #
    # hyprglass targets LAYER surfaces by namespace (layers { namespaces = … }).
    # For WINDOWS it exposes only a single GLOBAL `manage_window_blur` toggle —
    # there is NO per-class/per-window targeting in v0.7.0 (verified against the
    # built plugin's config keys). We deliberately do NOT flip manage_window_blur
    # (it would glass EVERY window, not just the terminal), so the Aero-glass
    # TERMINAL is done the compositor-native way instead: kitty renders a
    # translucent background (programs.kitty background_opacity, kitty dendrite)
    # and Hyprland's own blur (decoration:blur above, enabled globally) frosts
    # behind it — pinned to the kitty class by the windowrule below.
    plugin:hyprglass {
        layers {
            enabled = 1
            namespaces = aoide-bar, aoide-dock
            preset = glass
        }
    }

    # ── Aero-glass terminal — the kitty window rides the compositor blur ──────
    # kitty's background_opacity (0.82) makes only the cell BACKGROUND
    # translucent (glyphs stay opaque/crisp); Hyprland then blurs behind that
    # translucent surface, giving the frosted Win7 read. The whole-window opacity
    # rule keeps the FOCUSED terminal fully crisp (active 1.0) and adds a gentle
    # Aero fade when it loses focus (inactive 0.90). `windowrule` (v2 semantics,
    # the unified form on Hyprland 0.56) matches by regex class.
    windowrule = opacity 1.0 0.90, class:^(kitty)$
    windowrule = rounding 8, class:^(kitty)$
  '';

  # ── Hyprland keybinds for Aoide workflows ─────────────────────────────────
  # These are the binds the Quickshell facet depends on. Placed here so the
  # compositor facet owns all hyprctl-level wiring.
  hyprBindConfig = ''
    # ── Aoide keybinds ────────────────────────────────────────────────────
    # Launcher (shellbridge → AoideLauncher toggle)
    bind = SUPER, SPACE, exec, aoide shell launcher toggle

    # Lock screen
    bind = SUPER, ESCAPE, exec, aoide shell lock

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
    # vesktop/discord, gpu-screen-recorder) are dropped — the launcher and
    # the bar's power cell cover those seams. The hyprshot/satty binds
    # (SUPER+S / SUPER SHIFT+S) live in the screenshot dendrite, shipped
    # WITH the tools (modules/dendrites/screenshot.nix). dxflake's
    # media/brightness XF86 keys are also deliberately NOT bound (strict
    # window-management scope; the bar's volume cell owns audio by mouse) —
    # a known seam if hardware keys are wanted later.

    # Terminal (the kitty dendrite ships kitty)
    bind = SUPER, RETURN, exec, kitty

    # Window controls
    bind = SUPER, Q, killactive
    bind = SUPER, V, togglefloating
    bind = SUPER, F, fullscreen

    # Focus movement — arrows carry the full left/down/up/right set; H/J/K/L
    # complete the vim set (left/down/up/right). dxflake's SUPER+L was
    # blocked here by the Aoide lock bind, so the lock moved to SUPER+ESCAPE
    # above, freeing L — the full hjkl set is now live, matching arrows.
    bind = SUPER, left, movefocus, l
    bind = SUPER, down, movefocus, d
    bind = SUPER, up, movefocus, u
    bind = SUPER, right, movefocus, r
    bind = SUPER, H, movefocus, l
    bind = SUPER, J, movefocus, d
    bind = SUPER, K, movefocus, u
    bind = SUPER, L, movefocus, r

    # Move window (same directional scheme)
    bind = SUPER SHIFT, left, movewindow, l
    bind = SUPER SHIFT, down, movewindow, d
    bind = SUPER SHIFT, up, movewindow, u
    bind = SUPER SHIFT, right, movewindow, r
    bind = SUPER SHIFT, H, movewindow, l
    bind = SUPER SHIFT, J, movewindow, d
    bind = SUPER SHIFT, K, movewindow, u
    bind = SUPER SHIFT, L, movewindow, r

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

        # hyprglass — ABI-pinned to this Hyprland (see pkgs/hyprglass).
        # HM emits the `plugin = <path>` line; config in hyprNoteConfig.
        plugins = [ pkgs.hyprglass ];

        # ── systemd / Wayland env handoff (the session-assembly seam) ──────
        # This is what actually brings the desktop up. When enabled the HM
        # module emits, at the TOP of hyprland.conf:
        #   exec-once = dbus-update-activation-environment --systemd <vars>
        #               && systemctl --user start hyprland-session.target
        # hyprland-session.target BindsTo graphical-session.target, so this
        # single line imports HYPRLAND_INSTANCE_SIGNATURE / WAYLAND_DISPLAY /
        # XDG_CURRENT_DESKTOP into the systemd + D-Bus user environment and
        # pulls up graphical-session.target — which is what every Aoide user
        # service (quickshell, aoided, shellbridge) is `wantedBy`. Without it
        # a bare Hyprland launch would start the compositor and NOTHING else.
        #
        # The HM module defaults `systemd.enable` to true; we set it EXPLICITLY
        # so this contract survives a future edit to configType/settings that
        # might otherwise silently drop the handoff. `variables` keeps the
        # module default set (the vars listed above) — the clean HM path, not a
        # hand-rolled exec-once dbus call.
        systemd = {
          enable = true;
          variables = [
            "DISPLAY"
            "HYPRLAND_INSTANCE_SIGNATURE"
            "WAYLAND_DISPLAY"
            "XDG_CURRENT_DESKTOP"
            "XDG_SESSION_TYPE"
          ];
        };

        # Bake note + keybind config fragments into hyprland.conf.
        # mkBefore so note defaults land before any per-user overrides
        # (the quickshell facet appends its autostart with mkAfter).
        extraConfig = lib.mkBefore (hyprNoteConfig + hyprBindConfig);
      };

      # ── Polkit authentication agent ───────────────────────────────────────
      # security.polkit (the daemon) is pulled up by programs.hyprland/greetd,
      # but a polkit DAEMON without an AGENT means privileged GUI actions
      # (mounts, network edits, the aoided rebuild gate's future polkit prompt)
      # have nothing to present the authentication dialog — they silently fail.
      # hyprpolkitagent is the Hyprland project's QT/QML agent: the smallest
      # choice consistent with this compositor stack. Defined as an explicit
      # user service (journald-logged, Restart=on-failure) rather than relying
      # on the packaged unit landing on the systemd search path — same idiom as
      # the quickshell facet's shell service.
      systemd.user.services.hyprpolkitagent = {
        Unit = {
          Description = "Hyprland Polkit authentication agent (GUI privilege prompts)";
          PartOf = [ "graphical-session.target" ];
          After = [ "graphical-session.target" ];
          # Only meaningful once the Wayland session env is imported.
          ConditionEnvironment = "WAYLAND_DISPLAY";
        };
        Service = {
          ExecStart = "${pkgs.hyprpolkitagent}/libexec/hyprpolkitagent";
          Restart = "on-failure";
          RestartSec = 3;
        };
        Install.WantedBy = [ "graphical-session.target" ];
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
