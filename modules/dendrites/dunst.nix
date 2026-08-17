# modules/dendrites/dunst.nix — the dunst notification daemon.
#
# Dendrite shape v0 (CONTRACTS.md §2):
#   - one enable toggle: aoide.dunst.enable (off by default)
#   - carries its own dependencies: the dunst package (dunstctl/dunstify ride
#     along for the CLI and the herald center's `dunstctl history` poll) and
#     the daemon unit, via Home Manager's services.dunst module
#   - reads no other module — with ONE deliberate exception: config.aoide.livery,
#     the house design-token seam. Popups are a surface, and every surface's
#     dress flows from livery (the stylix facet reads the same option for the
#     same reason). The notif tier is nullOr-with-palette-fallback
#     (modules/nucleus/options.nix), resolved here, once.
#
# Why dunst owns org.freedesktop.Notifications now: the Quickshell
# NotificationServer (AoideNotifications.qml) is retired; dunst draws the
# themed popups, and the herald lives on as a dock notification CENTER that
# polls `dunstctl history` (song slot `herald-center`). The surfaces registry
# records this as notifications.owner = "dunst" (modules/facets/quickshell/
# default.nix), which also keeps Stylix's dunst/mako targets stood down so the
# dunstrc below is the only theming applied.
#
# ── The herald grammar, ported (khoa, 2026-08-17) ──────────────────────────
# This dunstrc dresses the popup as the retired popup card did — the Tuscan
# stele (git show d57205a~1:song/songbook/sonata/widgets/notifications.qml;
# the same grammar lives on at ledger scale in widgets/herald-center.qml).
# dunst is not QML: it draws one Pango-markup text block per notification
# plus a frame, an icon and a progress bar. The stele is therefore carved
# into the `format` string itself — the man page (dunst.5, v1.13.2) is the
# ground truth for every key used here; markup written in `format` is parsed
# regardless of the `markup` setting, which is the whole lever.
#
# Reading order (design/greek-grammar.md §4 "the herald tiers"), top down:
#     ❧ H E R A L D                 entablature: rust crown, ink wordmark
#     ══════════════                Tuscan frieze — the plain double rule
#     ┌─┤ program ├──               tier 1: program title in the frame label
#     ──────────────                the cleave — entablature/shaft boundary
#     Notification title            tier 2: bold serif, full ink
#       context                     tier 3: smaller, dimmer, indented
#     ( ・ω・)ノ                    ledger: urgency kaomoji
#     └─┤ normal ├── 𝄂              closing frame: urgency word, gold barline
#
# What ports 1:1 — the crown, frieze, frames, cleave, three text tiers, the
# per-urgency kaomoji + urgency word (via rule sections: `format` is a rule
# action and msg_urgency a rule filter; the special urgency_* sections do NOT
# accept `format`, so the three [herald-*] rules below carry the whole
# per-urgency treatment), critical-in-terracotta throughout, critical never
# times out, the 2px ink border (frame_width; gap_size > 0 frames every
# notification separately, exactly the retired 8px popup stack).
#
# What could NOT port, deliberately degraded (dunst has no conditionals, no
# animation, no per-line alignment):
#   - breathing critical pulse       -> persistent card + terracotta frame
#   - 1px inset signature keyline    -> one frame only; rust rides the glyphs
#   - right-aligned clock/tally      -> show_age_threshold stamps a native
#                                       age once a card lingers (the honest-
#                                       clock intent of b0da918)
#   - context hairline │ + smart     -> two-space indent; dunst renders the
#     OSC-9 "Title: message" split      spec fields as sent — the center
#     and desktop-entry parentTitle     still does the smart reading
#   - full-width stretching rules    -> fixed-length calligraphy, open-ended
#                                       on the right (a wrapped rule reads
#                                       broken; a short one reads deliberate)
#
# What dunst ADDS over the retired card (the point of the handover):
#   - a real progress bar (volume/brightness OSDs): gilded — gold fill in a
#     1px ink-framed trough, square corners
#   - sender icons, left, 24–32px — the center RE-admitted icons (2026-08-16,
#     khoa's own ask) and the popup follows it, not the retired card's
#     NO-ICONS stance
#   - duplicate stacking with an honest (2x) count, 20-deep history (the
#     exact window herald-center polls)
#   - actions: never drawn as chrome (fd4cb3a: the implicit "default" action
#     is click-anywhere, not a button) — show_indicators off; dunst's stock
#     mouse map already matches the card (left = dismiss, middle = invoke)
#
# Theming lands the way every Stylix-target app's does: baked at rebuild (the
# user-gated gate), not live from stage/livery.json. If live popup recoloring
# is ever wanted, `aoide livery emit file` is the ready seam.
#
# Enable with one line in hosts/yomi-strix/default.nix:
#   aoide.dunst.enable = true;
#
# hosts/ knows dendrites; dendrites never know hosts.
{ config, lib, pkgs, ... }:

let
  lv = config.aoide.livery;

  # livery's hexColor tolerates a missing leading '#'; dunstrc requires it.
  hex = c: if lib.hasPrefix "#" c then c else "#" + c;

  # notif tier: nullOr + palette fallback (options.nix's notifType); palette
  # tier fields always carry a value (closed tier, non-null defaults).
  notifBg = hex (if lv.notif.bg != null then lv.notif.bg else lv.palette.bg);
  notifFg = hex (if lv.notif.fg != null then lv.notif.fg else lv.palette.fg);
  notifUrgent = hex (if lv.notif.urgent != null then lv.notif.urgent else lv.palette.urgent);
  accent = hex lv.palette.accent;
  ink = hex lv.palette.fg;

  # The herald's signature: RUST, base16 base0F ("notifications' signature",
  # greek-grammar.md §5). Falls back the same way the stylix facet synthesizes
  # base0F when a song ships no base16 tier: to palette.urgent.
  rust = hex (if lv.base16 != null then lv.base16.base0F else lv.palette.urgent);

  # Ink opacities as Pango #RRGGBBAA — the retired card's withA() steps.
  # 0.95=F2 0.90=E6 0.85=D9 0.80=CC 0.70=B3 0.55=8C
  mono = "JetBrainsMono Nerd Font";

  # One stele per urgency. `sig` is the signature ink (rust; terracotta while
  # critical — "terracotta ink throughout"), `title` the tier-2 ink, `word`
  # the bottom-frame label (urgency word single-sourced there, b0da918),
  # `kao` the ledger kaomoji (the retired card's proven kana vocabulary).
  # Joined with a literal backslash-n: dunst replaces '\n' in format with a
  # real line break (dunst.5). No '%' or '&' may appear in the literals
  # (format variables / Pango entities); single quotes only, the generated
  # dunstrc wraps the value in double quotes.
  mkFormat =
    { sig, title, word, kao }:
    lib.concatStringsSep "\\n" [
      # entablature — rust crown ❧ (U+2767, the herald's clef stand-in), ink wordmark
      "<span font='Noto Serif Bold 15' foreground='${sig}'>❧</span><span font='Noto Serif 10' weight='600' foreground='${notifFg}'> H E R A L D</span>"
      # Tuscan frieze — the plain double rule IS the order's whole ornament
      "<span font='${mono} 8' foreground='${sig}B3'>════════════════════════════════════</span>"
      # tier 1: program title in the box-drawing frame label, open tassel right
      "<span font='${mono} 8' foreground='${sig}F2'>┌─┤ %a ├──────────</span>"
      # the cleave — the entablature/shaft boundary (greek-grammar.md §4)
      "<span font='${mono} 8' foreground='${sig}8C'>────────────────────────────────────</span>"
      # tier 2: the notification title, bold serif, full ink
      "<span font='Noto Serif Bold 10.5' foreground='${title}'>%s</span>"
      # tier 3: context — smaller, dimmer (0.85), indented; same voice, clearly second
      "<span font='Noto Serif 9' foreground='${notifFg}D9'>  %b</span>"
      # ledger — the urgency kaomoji in signature ink
      "<span font='${mono} 8' foreground='${sig}E6'>${kao}</span>"
      # closing frame — urgency word in the label, gold 𝄂 barline ends the stele
      "<span font='${mono} 8' foreground='${notifFg}CC'>└─┤ ${word} ├</span><span font='${mono} 8' foreground='${sig}8C'>──────────── </span><span font='Noto Music 12' foreground='${accent}'>𝄂</span>"
    ];

  # Shared per-urgency dress: opaque marble stele, ink text, gilded gauge.
  steleColors = {
    background = notifBg;
    foreground = notifFg;
    highlight = accent; # progress-bar fill — gold, the one gauge ink
  };
in
{
  options.aoide.dunst.enable =
    lib.mkEnableOption "dunst notification daemon (the herald's delivery backend)";

  config = lib.mkIf config.aoide.dunst.enable {
    environment.systemPackages = [ pkgs.dunst ];

    # HM's module owns the whole daemon lifecycle: dunstrc generation from
    # `settings`, a Type=dbus unit holding BusName=org.freedesktop.Notifications
    # bound to the graphical session, D-Bus activation file, and a dunstrc
    # reload trigger. Hand-rolling that (clipboard.nix style) would buy nothing.
    home-manager.users.${config.aoide.user}.services.dunst = {
      enable = true;
      settings = {
        global = {
          # ── the stele: square, serif, bottom-right — where and what the
          # retired popup surface drew (360px stack, 12/8 corner margins,
          # 8px breath between cards; gap_size > 0 also gives every card
          # its OWN 2px ink frame and retires the separator entirely) ──────
          corner_radius = 0;
          width = 360;
          height = "(0, 600)";
          origin = "bottom-right";
          offset = "(12, 8)";
          frame_width = 2;
          frame_color = ink;
          gap_size = 8;
          padding = 10;
          horizontal_padding = 10;
          font = "Noto Serif 11";

          # markup in `format` is always parsed (dunst.5); `full` additionally
          # honors spec markup senders put in the body — dunst is a rich
          # renderer, unlike the retired v1 card, and may as well act like one.
          markup = "full";

          # ── icons: the center's re-admission, popup edition — the sender's
          # provided icon, left of the stele text, 24–32px, 8px off the text
          # (the center's icon-box metrics) ─────────────────────────────────
          icon_position = "left";
          min_icon_size = 24;
          max_icon_size = 32;
          text_icon_padding = 8;

          # ── ported behaviors ─────────────────────────────────────────────
          # the honest clock: a lingering card (critical persists) gets a
          # native age stamp after a minute — b0da918's no-stale-time rule
          show_age_threshold = 60;
          # fd4cb3a: the implicit "default" action is click-anywhere, never
          # chrome — no (A) indicators; dunst's stock mouse map already
          # matches the card (left dismiss, middle invoke)
          show_indicators = false;
          # the exact window herald-center polls out of `dunstctl history`
          history_length = 20;

          # ── the gilded gauge (progress notifications: volume/brightness) —
          # gold fill (highlight, per-urgency below) in a 1px ink-framed
          # trough, square, full text width ────────────────────────────────
          progress_bar_height = 8;
          progress_bar_frame_width = 1;
          progress_bar_max_width = 336;
        };

        # ── one rule per urgency — filter msg_urgency, carry the WHOLE
        # treatment (the special urgency_* sections accept colors only, not
        # `format`; these rules are the single per-urgency source) ──────────
        herald-low = steleColors // {
          msg_urgency = "low";
          frame_color = ink;
          timeout = "5s"; # house default when the sender leaves it to us
          format = mkFormat {
            sig = rust;
            title = notifFg;
            word = "low";
            kao = "(´ω｀)"; # at ease, unhurried
          };
        };
        herald-normal = steleColors // {
          msg_urgency = "normal";
          frame_color = ink;
          timeout = "5s";
          format = mkFormat {
            sig = rust;
            title = notifFg;
            word = "normal";
            kao = "( ・ω・)ノ"; # attentive, on the case
          };
        };
        # critical: terracotta ink throughout, never times out — the summons
        # persists until dismissed (the breathing pulse has no dunst analog;
        # the standing terracotta frame is its still form)
        herald-critical = steleColors // {
          msg_urgency = "critical";
          frame_color = notifUrgent;
          timeout = 0;
          format = mkFormat {
            sig = notifUrgent;
            title = notifUrgent;
            word = "critical";
            kao = "(ノ｀ｏ´)ノ"; # alarmed — a critical herald
          };
        };
      };
    };
  };
}
