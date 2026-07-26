# modules/facets/stylix/default.nix — the Stylix facet (baked fan-out).
#
# Stylix is Aoide's "recording" side (concepts/Design-Tokens, entities/Stylix):
# ONE base16 scheme + fonts/cursor/wallpaper feed Stylix, which themes every
# nix-manageable target — GTK/Qt, terminal, editors, boot. The live side
# (stage/tokens.json + hyprctl + OSC) is the token package's job; this facet is
# the baked half. Both derive from the same `aoide.tokens`, so preview and
# adopted state cannot diverge ("zero drift").
#
# Contract discipline (CONTRACTS.md §2, docs/BUILD.md):
#   * A facet reads ONLY `aoide.tokens` + `aoide.surfaces`. No other module.
#   * It applies the component-tier null→palette fallback ITSELF (§1) — the
#     option system stores null; the facet resolves it.
#   * It reads the `aoide.surfaces` ownership registry and stands down (disables
#     its own derivation) for any surface a quickshell facet already owns
#     (overlap resolution). Tolerates the registry being empty (Wave-1 order).
#   * Never reads a `song/` runtime path at build time (checks.no-song-read).
#
# Keyed on `aoide.facets.stylix.enable`. The host flips it in one line
# (`aoide.facets.stylix.enable = true;`), per docs/BUILD.md.
{
  config,
  options,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.aoide.facets.stylix;
  t = config.aoide.tokens;

  # Stylix rides as a NixOS module only when its input is present (mkHost adds
  # it optionally). Gate on the OPTION being declared — reading `options` (not
  # `config`) avoids the config→config infinite recursion, so a minimal eval
  # without the stylix module stays clean.
  stylixPresent = options ? stylix;

  # ── hex helpers ───────────────────────────────────────────────────────────
  # Stylix's base16Scheme attrset wants bare hex (no leading '#'); the token
  # option type is permissive (`#?[0-9a-fA-F]{6}`), so normalise.
  stripHash = c: lib.removePrefix "#" c;

  # ── palette ───────────────────────────────────────────────────────────────
  p = t.palette;

  # ── component-tier null→palette fallback (CONTRACTS.md §1) ─────────────────
  # Applied HERE by the facet, exactly as the token package's resolver applies
  # it for the live side — identical rules, so both fan-outs agree.
  fb = value: fallback: if value != null then value else fallback;

  # Resolved component-tier values (null → palette). Applied HERE by the facet,
  # exactly as the token package's resolver applies it for the live side —
  # identical rules, so both fan-outs agree. These feed the base16 anchoring
  # below, so the component overrides genuinely reach the baked theme (not just
  # the palette): e.g. `bar.bg` drives the "lighter background" slot Stylix uses
  # for status surfaces, and `window.border` drives the accent slot.
  resolved = {
    bar = {
      bg = fb t.bar.bg p.bg;
      fg = fb t.bar.fg p.fg;
      accent = fb t.bar.accent p.accent;
    };
    notif = {
      bg = fb t.notif.bg p.bg;
      fg = fb t.notif.fg p.fg;
      urgent = fb t.notif.urgent p.urgent;
    };
    window = {
      border = fb t.window.border p.accent;
      borderInactive = fb t.window.borderInactive p.bg;
    };
  };

  # ── base16 scheme from the v0 palette ─────────────────────────────────────
  # v0 palette is base16-closed (options.nix): bg=base00, fg=base05,
  # accent=base0D, urgent=base08. We synthesise a full base00..base0F so Stylix
  # has a complete scheme; the four authored anchors drive the salient slots and
  # the resolved component tier informs the surface-adjacent slots. (v1's
  # design-system work replaces this synthesis with a real 16-colour derivation.)
  scheme = {
    base00 = stripHash p.bg; # background
    base01 = stripHash resolved.bar.bg; # lighter bg (status surfaces) ← bar.bg
    base02 = stripHash resolved.window.borderInactive; # selection bg ← window.borderInactive
    base03 = stripHash p.fg; # comments
    base04 = stripHash p.fg; # dark fg
    base05 = stripHash p.fg; # default fg
    base06 = stripHash p.fg; # light fg
    base07 = stripHash p.fg; # lightest fg
    base08 = stripHash resolved.notif.urgent; # red / urgent ← notif.urgent
    base09 = stripHash p.accent; # orange
    base0A = stripHash p.accent; # yellow
    base0B = stripHash p.accent; # green
    base0C = stripHash p.accent; # cyan
    base0D = stripHash resolved.window.border; # blue / accent ← window.border
    base0E = stripHash resolved.bar.accent; # magenta ← bar.accent
    base0F = stripHash p.urgent; # brown
  };

  # A minimal, deterministic wallpaper: a solid-colour PNG from palette.bg.
  # Stylix wants an `image`; generating one keeps the baked path buildable
  # without shipping a binary asset and with NO song/ read. The rice engine
  # (Wave-1 rime) overrides this per song with a real wallpaper — hence
  # mkDefault. ImageMagick is a pure build-time input.
  wallpaper = pkgs.runCommand "aoide-wallpaper.png" { } ''
    ${pkgs.imagemagick}/bin/magick -size 1920x1080 "xc:#${stripHash p.bg}" "$out"
  '';

  # ── surface-ownership overlap resolution ──────────────────────────────────
  # Read the registry (tolerate it empty). Any surface owned by a NON-stylix
  # owner (the Quickshell facet, Agent C) is one Stylix must not also drive:
  # for each such surface Stylix stands down for the target(s) that would
  # collide (concepts/Design-Tokens, "Stylix Overlap Resolution").
  #
  # Agent C's registry (confirmed) owns these surfaces under "quickshell":
  #   bar · notifications · launcher · osd · lockscreen · greeter · wallpaper ·
  #   agentWidgets
  # We map each onto the concrete Stylix target(s) it displaces. Quickshell
  # renders bar/launcher/osd/agentWidgets in its own QML — Stylix has no target
  # for those, so they contribute nothing (empty lists). The ones with a real
  # Stylix target are notifications / lockscreen / greeter.
  surfaces = config.aoide.surfaces or { };
  ownedByOthers = lib.filterAttrs (_: s: (s.owner or "") != "stylix") surfaces;
  ownedSurfaceNames = lib.attrNames ownedByOthers;

  # Surface → the Stylix targets Aoide would otherwise theme for it. Only names
  # that exist as real `stylix.targets.<name>` on this Stylix version are kept
  # (guarded below), so an unknown/renamed target can never break eval.
  surfaceToStylixTargets = {
    notifications = [
      "mako"
      "dunst"
    ];
    lockscreen = [
      "gtklock"
      "hyprlock"
    ];
    greeter = [ "gnome" ];
    # bar / launcher / osd / agentWidgets / wallpaper: no colliding Stylix target
    # (quickshell owns these purely in QML). `wallpaper` is handled separately
    # below (stylix.image policy), not by disabling a target.
  };

  # Every name in `surfaceToStylixTargets` is a REAL upstream Stylix target
  # (mako/dunst/gtklock/hyprlock/gnome all declare `stylix.targets.<name>.enable`
  # unconditionally — setting it false is a safe no-op even when the underlying
  # program isn't installed). So we can disable them directly without probing
  # option existence (which is unreliable through a submodule's freeform type).
  disabledTargets = lib.unique (
    lib.flatten (map (n: surfaceToStylixTargets.${n} or [ ]) ownedSurfaceNames)
  );
  targetDisableAttrs = lib.genAttrs disabledTargets (_: {
    enable = lib.mkForce false;
  });

  # `wallpaper` surface policy (deterministic, documented): the quickshell
  # wallpaper LAYER supersedes at render time, but Stylix's `image` remains the
  # single source the base16 context is derived from and the fallback when the
  # quickshell layer is absent. So we ALWAYS set `stylix.image` (as mkDefault);
  # ownership of the `wallpaper` surface does NOT suppress it. This keeps the
  # baked scheme coherent and lets quickshell paint over it live.
  wallpaperOwnedElsewhere = builtins.elem "wallpaper" ownedSurfaceNames;
in
{
  options.aoide.facets.stylix.enable =
    lib.mkEnableOption "the Stylix baked-theme facet (base16 fan-out from aoide.tokens)";

  # The baked Stylix settings. Only emitted when the stylix input is present:
  # `lib.optionalAttrs stylixPresent` keeps the `stylix` KEY out of `config`
  # entirely on a minimal eval where the option is undeclared, so the facet
  # evals clean whether stylix rides or not (mkIf alone still leaves a
  # definition on the undeclared path). On the real host mkHost always adds the
  # stylix module, so this is present.
  config = lib.mkIf cfg.enable (
    lib.optionalAttrs stylixPresent {
      stylix = {
        enable = true;
        polarity = lib.mkDefault "dark";

        # The ONE base16 scheme — the baked fan-out's single source.
        base16Scheme = scheme;

        # Wallpaper: a deterministic solid-colour default; the rice engine
        # overrides per song. mkDefault keeps it host/rice-overridable. We set
        # this EVEN WHEN quickshell owns the `wallpaper` surface
        # (`wallpaperOwnedElsewhere` = ${lib.boolToString wallpaperOwnedElsewhere}):
        # Stylix's image stays the base-context source and the fallback; the
        # quickshell wallpaper layer supersedes it live at render time.
        image = lib.mkDefault wallpaper;

        # Cursor: a stock theme; v0 tokens carry no cursor field yet, so this is
        # a sane default the rice engine / host can override.
        cursor = lib.mkDefault {
          package = pkgs.adwaita-icon-theme;
          name = "Adwaita";
          size = 24;
        };
      }
      # Stand down for surfaces owned by other facets (quickshell). Empty today;
      # structurally correct so the overlap check gains teeth as Agent C lands.
      // lib.optionalAttrs (disabledTargets != [ ]) {
        targets = targetDisableAttrs;
      };
    }
  );
}
