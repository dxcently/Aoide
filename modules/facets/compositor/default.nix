# modules/facets/compositor/default.nix — Hyprland compositor facet.
#
# Wires Hyprland as the NixOS Wayland compositor and applies compositor-side
# livery live via hyprctl. Livery values (gaps, radius, borders, blur)
# are baked into the Hyprland config at build time so they take effect on
# session start; the livery engine (crates/song) can re-dispatch
# them live via hyprctl during a rehearsal (preview) pass.
#
# Scope: LOOK + session plumbing only. Everything host-invariant — keybinds,
# input devices, tiling layout, misc, behavioural window rules — lives in
# modules/dendrites/hyprland.nix so a re-rice cannot disturb it. The window
# rules that remain HERE (kitty opacity/rounding) are appearance, hence
# livery's business; see that dendrite's header for the full split.
#
# IPC socket: exposes the Hyprland IPC socket path for shellbridge to consume.
# shellbridge uses it to track windows and dispatch focus commands
# (hyprctl dispatch focuswindow) backing the Terminal-Commander session-jump
# flow (concepts/Desktop-Architecture).
#
# Reading discipline (CONTRACTS.md §1):
#   - Reads ONLY aoide.livery (palette + component tiers) and
#     aoide.arrangement (declared widget/surface types) — the enumerated,
#     closed facet whitelist (AGENTS.md house rule 5).
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
  # Read-side venue recolour (CONTRACTS.md §1, override tier): resolve rewrites
  # colours equal to an overridden anchor's authored value, in one pass, with
  # no option-system recursion — the option itself stays inert either way.
  t = (import ../../../lib/livery.nix { inherit lib; }).resolve config.aoide.livery;
  arr = config.aoide.arrangement;

  # ── Component-tier fallback helpers ────────────────────────────────────────
  # No hex lives here by design (CONTRACTS.md §1/§5): the facet is host- and
  # song-agnostic, so border colour is ALWAYS a note read, never a literal.
  # This IS the substitution seam — window.border/window.borderInactive (or
  # their palette.accent/palette.bg fallback) are set per-song in that song's
  # rice.nix (e.g. song/songbook/sonata/rice.nix), which maps its own base16
  # roles onto the component tier. A song owns that assignment; this facet only
  # reads it, so changing a rendered border colour is always a song-side edit.
  windowBorder = if t.window.border != null then t.window.border else t.palette.accent;
  windowBorderInactive =
    if t.window.borderInactive != null then t.window.borderInactive else t.palette.bg;

  # ── Greeter colour helpers ────────────────────────────────────────────────
  # ly takes colours as 32-bit `0xSSRRGGBB`: a styling byte ahead of the RGB
  # (00 plain, 01 bold), with `full_color` on by default. So the palette's own
  # hex goes in unmapped — no eight-colour approximation, and no second colour
  # vocabulary in this facet. Same read-side discipline as windowBorder above:
  # a note read, never a literal.
  lyPlain = hex: "0x00${lib.removePrefix "#" hex}";
  lyBold = hex: "0x01${lib.removePrefix "#" hex}";

  # ── Derived geometry values ───────────────────────────────────────────────
  # v0 geometry tier (additive-optional, CONTRACTS.md §1): a song MAY set
  # aoide.livery.geometry.*; every field is nullOr and falls back to the
  # opinionated defaults below when unset (component-tier fallback pattern,
  # same as windowBorder/windowBorderInactive above). No song sets geometry
  # today, so these fallbacks ARE the immutable baseline in practice.
  geo = t.geometry;
  gapOuter = if geo.gapsOut != null then geo.gapsOut else 8;
  gapInner = if geo.gapsIn != null then geo.gapsIn else 6;
  borderWidth = if geo.borderSize != null then geo.borderSize else 2;
  # Edged windows (the User, with the bw border key): square corners — the
  # dxflake read. The Pantheon wireframe language wants hard outlines too.
  rounding = if geo.rounding != null then geo.rounding else 0; # window corner radius (px)
  blurEnabled = if geo.blurEnabled != null then geo.blurEnabled else true;
  blurPasses = if geo.blurPasses != null then geo.blurPasses else 3;
  blurSize = if geo.blurSize != null then geo.blurSize else 8;

  # ── Declared widget-type registry → compositor layerrules (v1) ───────────
  # aoide.arrangement.widgets (nucleus/options.nix, Phase 1) is the registry a
  # song uses to declare a brand-new surface-kind widget TYPE. It is empty
  # for every song until Phase 3/4/6 land (Sonata, the only committed song
  # today, declares nothing) — mapAttrsToList over {} yields [ ], so every
  # derived value below is [ ] / "" in practice right now: a structural
  # no-op, not an accident of which song happens to be active.
  #
  # Each entry's namespace is its declared `namespace` field, or else
  # "aoide-<slot>" from the attribute key (the option doc's contract — a
  # plain nix default can't see its own key, so the consumer derives it).
  # The layerrule idiom matches the aoide-dock/launcher/powermenu/calendar
  # rules above exactly: blur on + ignore_alpha when blurred, blur off
  # (pinned, same as aoide-calendar) when not — never an unmatched
  # namespace left to a future blanket rule's mercy.
  #
  # NOTE on `layer` (overlay/top): checked against the built Hyprland
  # source (src/desktop/rule/layerRule/LayerRule.cpp) —
  # Desktop::Rule::CLayerRule::matches switches solely on
  # RULE_PROP_NAMESPACE; there is no per-layer match criterion `layerrule`
  # can target. `layer` therefore has no effect on THIS file's generation;
  # it's the QML runtime's WlrLayershell.layer choice (Phase 4), not
  # compositor-facet business.
  # Filter to kind == "surface" first: a `dock` entry has no layer surface
  # of its own (it mounts as an Item inside aoide-dock's PanelWindow), so
  # it must never generate a layerrule/hyprglass namespace here.
  declaredWidgets = lib.mapAttrsToList (slot: w: {
    namespace = if w.namespace != null then w.namespace else "aoide-${slot}";
    inherit (w) blur;
  }) (lib.filterAttrs (_: w: w.kind == "surface") arr.widgets);

  widgetLayerRules = lib.concatMapStrings (
    e:
    if e.blur then
      ''
        layerrule = blur on, match:namespace ${e.namespace}
        layerrule = ignore_alpha 0.05, match:namespace ${e.namespace}
      ''
    else
      "layerrule = blur off, match:namespace ${e.namespace}\n"
  ) declaredWidgets;

  # Same glass-namespace idiom as aoide-dock/launcher/powermenu: only
  # blurred widgets join the hyprglass namespace list (an unblurred entry
  # stays deliberately absent, same reasoning as aoide-calendar above).
  widgetGlassNamespaces = lib.concatStringsSep ", " (
    map (e: e.namespace) (lib.filter (e: e.blur) declaredWidgets)
  );

  # ── Hyprland config fragment — notes baked in at build time ──────────────
  # The livery emitter (crates/song) re-runs hyprctl keyword dispatch
  # during rehearsal to live-patch these values without a rebuild.
  hyprNoteConfig = ''
    # ── Aoide notes — compositor facet ───────────────────────────────────
    # Generated from aoide.livery at build time; live-patched by the livery
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

    # Glass for the quickshell surfaces. For THIS rice (sonata) the bar
    # (aoide-bar) is OPAQUE marble and NOT glassed. The center-left dock
    # (aoide-dock) layer IS blurred/glassed, but its panels are mostly opaque
    # marble — only the TERMINALS temple is translucent, so the blur + hyprglass
    # frost THROUGH it while the opaque Conductor/Meters/Power panels hide it.
    # The launcher stays frosted glass. The wallpaper is never blurred.
    # blur_popups frosts the bar's popouts. ignore_alpha keeps transparent
    # regions from rendering as a grey blur stripe.
    # aoide-launcher: the summoned launcher pane rides the same frosted glass as
    # the dock (it is a Pantheon pane too). Blur + ignore_alpha so its cream
    # glass frosts over whatever window it covers and its transparent scrim/edges
    # don't render as a grey blur stripe.
    # aoide-powermenu: the Exodos powermenu (AoideExodos.qml) — the same
    # summoned-overlay glass recipe as the launcher: its full-screen ink scrim
    # and translucent steles frost over the desktop behind.
    layerrule = blur on, match:namespace aoide-dock
    layerrule = blur on, match:namespace aoide-launcher
    layerrule = blur on, match:namespace aoide-powermenu
    layerrule = ignore_alpha 0.05, match:namespace aoide-dock
    layerrule = ignore_alpha 0.05, match:namespace aoide-launcher
    layerrule = ignore_alpha 0.05, match:namespace aoide-powermenu
    # blur_popups extends the glass to the bar's PopupWindow children (the
    # gadget popouts) — same 0.5x snake_case rework spelling as ignore_alpha.
    layerrule = blur_popups on, match:namespace aoide-bar
    # aoide-calendar: the calendar popout rides its OWN layer surface
    # (SteleLayerPopout — not an xdg_popup of aoide-bar) precisely so
    # blur_popups above cannot reach it: its papyrus sheet keeps an opaque
    # marble frame but cuts a transparent window over the day grid, and that
    # window must show the desktop CRISPLY (the User, 2026-08-13: "no blur").
    # An unmatched layer namespace gets no blur by default — this rule pins
    # the exclusion EXPLICITLY so a future blanket layer rule can't silently
    # frost it. Deliberately absent from the hyprglass namespaces below for
    # the same reason.
    layerrule = blur off, match:namespace aoide-calendar

    # aoide.arrangement.widgets (declared widget-type registry, v1): one
    # layerrule pair (or a pinned blur-off) per registered slot, same idiom
    # as the aoide-* rules above. Empty today for every song — expands only
    # once a song's rice.nix actually populates .widgets (Phase 3/4/6).
    ${widgetLayerRules}
    # hyprglass (pkgs/hyprglass, loaded via the HM plugins list below):
    # Liquid Glass on the quickshell surfaces, ON TOP of the blur+gloss —
    # refraction/fresnel the flat gradient can't fake. Same namespaces as
    # the layerrules; the wallpaper surface stays untouched.
    #
    # hyprglass targets LAYER surfaces by namespace (layers { namespaces = … }).
    # For WINDOWS it exposes only a single GLOBAL `manage_window_blur` toggle —
    # there is NO per-class/per-window targeting in v0.7.0 (verified against the
    # built plugin's config keys). The User asked for hyprglass on the terminals
    # (and "any transparent layer") too, so we DO flip manage_window_blur here:
    # this desktop is terminal-centric and the glass shader only paints visible
    # TRANSLUCENT content (it discards fully-transparent/opaque-covered
    # fragments), so opaque windows (Firefox &c.) are untouched while the
    # frosted kitty gains hyprglass refraction/fresnel ON TOP of Hyprland's own
    # blur. The `light` preset override brightens the glass under sonata's light
    # polarity (a whiter frost, per the same directive).
    plugin:hyprglass {
        manage_window_blur = 1
        light {
            glass_opacity = 0.82
        }
        layers {
            enabled = 1
            namespaces = aoide-dock, aoide-launcher, aoide-powermenu${
              lib.optionalString (widgetGlassNamespaces != "") ", ${widgetGlassNamespaces}"
            }
            preset = glass
        }
    }

    # ── Aero-glass terminal — the kitty window rides the compositor blur ──────
    # kitty's background_opacity (0.86) makes only the cell BACKGROUND
    # translucent (glyphs stay opaque/crisp); Hyprland then blurs behind that
    # translucent surface, giving the frosted Win7 read. The whole-window opacity
    # rule keeps the FOCUSED terminal fully crisp (active 1.0) and adds a gentle
    # Aero fade when it loses focus (inactive 0.80, deepened from 0.90 so an
    # unfocused terminal reads as visibly receded against a focused one).
    # Hyprland 0.56 matches with the `match:<prop> <value>` form (same as the
    # layerrules above); the old `class:^(kitty)$` form is rejected ("invalid
    # field ... missing a value"). Terminals ONLY — this is not a global
    # inactive_opacity: media/image/video/browser windows carry arbitrary,
    # non-theme-matched content and must stay 1.0/1.0 by never matching a rule.
    # LEGIBILITY FLOOR (the User + Fable advisory): 0.80 keeps unfocused terminal
    # text over the marble field at ≈3.7:1, still glanceable. Fable's floor is
    # 0.75 (≈3.2:1); 0.70 breaks readability outright. If a live vision check
    # ever finds unfocused terminal text hard to read, raise this toward 0.85 —
    # never drop below 0.75. The lines/text stay clean and readable; the
    # transparency serves that, not the other way around.
    windowrule = opacity 1.0 0.80, match:class kitty
    # Edged everywhere (the User): hard square corners on the terminal too — the
    # global decoration rounding is already 0, so this pins kitty to match
    # (the earlier `rounding 3` softened only the terminal; now nothing rounds).
    windowrule = rounding 0, match:class kitty
  '';

  # NOTE: keybinds, input devices, tiling layout, misc, and BEHAVIOURAL window
  # rules are NOT here — they moved to modules/dendrites/hyprland.nix, which
  # owns everything that must survive a re-rice untouched. This facet keeps
  # only the livery-derived look above plus the session plumbing below. The
  # appearance rules (kitty opacity/rounding, the aoide-* layerrules) stay
  # here on purpose: they are livery's business, not behaviour.
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

        # Bake the livery fragment into hyprland.conf. mkBefore (order 500) so
        # livery defaults land first; the hyprland dendrite's behaviour block
        # and the screenshot dendrite's binds follow at the default order
        # (1000). One `lines` option, several writers. (The quickshell facet
        # writes nothing here — it autostarts the shell as a systemd user
        # service, not an exec-once.)
        extraConfig = lib.mkBefore hyprNoteConfig;
      };

      # ── Polkit authentication agent ───────────────────────────────────────
      # security.polkit (the daemon) is pulled up by programs.hyprland,
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

    # ── ly (login greeter) ─────────────────────────────────────────────────
    # The greeter: a TUI display manager on tty1 that authenticates through
    # PAM and launches the Hyprland wayland-session `programs.hyprland`
    # already registers. Session plumbing, which is this facet's other half.
    #
    # It replaces greetd, wired here as a stub for a Quickshell greeter that
    # was never written — no `AoideGreeter.qml` ever existed. That stub put
    # Hyprland in greetd's `default_session`, the GREETER slot rather than
    # `initial_session`, so the desktop came up with no authentication step
    # at all and logind classed the whole session `greeter`. ly restores the
    # login prompt, and the `user` class follows from PAM registering a real
    # login.
    #
    # Riced by palette read alone. Everything else — animation, clock, box
    # title, key hints — stays at ly's defaults: a song dresses the greeter
    # through `aoide.livery`, and none of those are a taste decision this
    # facet gets to make on a song's behalf.
    services.displayManager.ly = {
      enable = true;
      settings = {
        bg = lyPlain t.palette.bg;
        fg = lyPlain t.palette.fg;
        border_fg = lyPlain t.palette.accent;
        # Bold, as ly's own default red is — an error stays emphatic.
        error_fg = lyBold t.palette.urgent;
      };
    };

    # Upstream gap, not a preference. nixpkgs' ly module builds
    # `systemd.services.display-manager` through the deprecated
    # `services.displayManager.generic` path, and neither that module nor the
    # shared display-manager module ever gives the unit a `wantedBy` — so it
    # is installed and never started, and the machine boots to a bare tty1
    # with no greeter. greetd did not have the problem because its own module
    # carries `wantedBy = [ "graphical.target" ]` and aliases
    # display-manager.service onto it. Supply the missing link; drop this the
    # day the ly module carries it.
    systemd.services.display-manager.wantedBy = [ "graphical.target" ];
  };
}
