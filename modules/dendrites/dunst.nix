# modules/dendrites/dunst.nix — the dunst notification daemon.
#
# Dendrite shape v0 (CONTRACTS.md §2):
#   - one enable toggle: aoide.dunst.enable (off by default)
#   - carries its own dependencies: the dunst package (dunstctl/dunstify ride
#     along for the CLI, the herald center's `dunstctl history` poll and the
#     permission summons `aoide graph permit` raises), libnotify for the
#     stock `notify-send` client other aoide code already calls, and the
#     daemon unit itself via Home Manager's services.dunst module
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
# dunst is not QML: per notification it draws ONE Pango text block, one frame,
# one icon and one progress bar. The stele is therefore carved into the
# `format` string itself — markup written in `format` is parsed regardless of
# the `markup` setting, which is the whole lever. dunst.5 (v1.13.2) is the
# ground truth for every key used here; everything below was rendered and
# looked at on a live daemon before it was written down.
#
# Reading order, top down:
#     ❧ H E R A L D          [ notify ]   entablature: rust crown, ink wordmark,
#     ══════════════════════════════════  Tuscan frieze — the plain double rule
#     ┌─┤ program ├───────────────         tier 1: program in the frame label
#     ──────────────────────────────────  the cleave — entablature/shaft line
#     Notification title                  tier 2: bold serif, full ink
#     │ context                           tier 3: smaller, dimmer, hairline
#     ──────────────────────────────────  the ledger rule
#     ( ・ω・)ノ  [ 60%]                   ledger: urgency kaomoji, gauge tally
#     └─┤ normal ├─────────── 𝄂 ┘         closing frame: urgency word, barline
#
# Sizes are the retired card's pixel metrics converted at 96dpi (pt = px*0.75,
# and the rig runs scale 1.0): 20px crown → 15pt, 13px wordmark → 9.75pt,
# 11px mono → 8.25pt, 10px mono → 7.5pt, 14px title → 10.5pt, 12px context →
# 9pt, 16px 𝄂 → 12pt. `letter_spacing='2304'` is the card's `letterSpacing: 3`
# (3px → 2.25pt → 2304 Pango units) — a real 1:1 port, not faked with spaces.
#
# What ports 1:1 — the ❧ crown and the `[ notify ]` order tag, the frieze, both
# box-drawing frames, the cleave, the three text tiers with the │ hairline tick
# on the context, the ledger rule, the per-urgency kaomoji (the card's own
# proven kana vocabulary) and urgency word, the gold 𝄂 + rust ┘ closing the
# stele with the same small gap the card left between them, terracotta ink
# throughout while critical, critical never timing out, 2px ink frame, radius 0
# everywhere, 10px padding, 360px width, and `gap_size` 8 giving every card its
# own frame — the retired 8px popup stack exactly. Also 1:1, and the reason the
# card survives hostile senders: BOTH text tiers are plain text
# (`markup = no`), which is the card's own `textFormat: Text.PlainText`. An
# earlier pass ran `markup = full`; a body carrying `<done>` or any unbalanced
# tag then failed Pango parsing and dunst fell back to rendering the WHOLE
# format as unstyled plain text — measured, every span lost, every rule
# wrapped. `no` escapes sender text instead, and the format's own markup is
# still parsed. Untrusted text is data, never re-interpreted.
#
# What could NOT port, deliberately degraded (dunst has no conditionals worth
# the name, no animation, no per-line alignment, no clock):
#   - cast shadow                    -> nothing; the frame carries the edge
#   - 1px inset rust keyline         -> nothing; one frame only, kept ink (the
#                                       card's OUTER border), rust rides the ink
#   - breathing critical pulse       -> the standing terracotta frame is its
#                                       still form
#   - the gold arrival clock, right  -> NOTHING. There is no time placeholder,
#                                       and `show_age_threshold`'s native stamp
#                                       is appended OUTSIDE the format's markup
#                                       in the default font, where it wraps and
#                                       tears the closing frame open (measured)
#                                       — hence -1. The honest clock lives on in
#                                       herald-center, which reads each entry's
#                                       real timestamp out of history.
#   - the honest (Nx) duplicate count-> hidden, same defect mirrored: dunst
#                                       PREPENDS it outside the markup and it
#                                       shoves `[ notify ]` onto a second line.
#                                       Stacking itself stays on; only the
#                                       numeral is dropped.
#   - full-width stretching rules    -> fixed-length runs measured against the
#                                       336px text band (48 cells at 8.25pt).
#                                       Every rule but one lands flush right.
#   - a ┐ closing the top frame      -> impossible: `%a` is variable, dunst has
#                                       no padding. Its tail is a fixed 28 cells
#                                       (fits a 17-char sender before wrapping);
#                                       the bottom frame, whose label IS static,
#                                       does close on ┘.
#   - right-aligned anything         -> only the entablature tally, where BOTH
#                                       operands are static, so a tuned space
#                                       run is exact rather than fragile
#   - smart OSC-9 "Title: message"   -> dunst renders the spec fields as sent;
#     splitting, desktop-entry names    the center still does the smart reading
#   - indicate_hidden's "(N more)"   -> off; that synthetic card bypasses
#                                       `format` entirely and cannot wear the
#                                       dress. `notification_limit` keeps the
#                                       visible stack to what fits the screen;
#                                       the overflow waits its turn, and history
#                                       loses nothing.
#
# What dunst ADDS over the retired card (the point of the handover):
#   - REAL images. `image-path`/`image-data` land in the icon slot, and the slot
#     is on TOP (not left): a raster plate above the pediment costs the
#     calligraphy no width, where a left icon would have shortened every rule
#     in the card by 40px whether or not a sender supplied one. 24–48px, square
#     (`icon_corner_radius = 0`, radius 0 house-wide) — was 24–96 until a live
#     96px app icon dwarfed its own stele (khoa, 2026-08-17: "gigantic"; the
#     plate is a mark, not a poster). The max IS the proportion rule — a
#     1920x1080 screenshot lands as a 48x27 plate, album art as 48x48, and
#     nothing can outgrow the stele it sits on.
#   - a gilded gauge for progress senders: gold fill in a 1px ink-framed square
#     trough, the full 336px text width. dunst always draws the bar after the
#     text, so it sits BELOW the closing barline — read it as the stele's
#     plinth. `%p` also rides the ledger line in gold and vanishes when the
#     sender set no value.
#   - the SUMMONS: an agent permission prompt is not a toast (see the
#     herald-summons rule). It is the one card whose gestures DO something —
#     left-click approves, middle-click denies, and `aoide graph permit` types
#     the verdict back into the waiting session. Both gestures were driven end
#     to end against a live conducted session before this was written down.
#     The approve/deny controls live IN the closing frame's label slots (each
#     carrying its own gesture name) — see the closing-frame comment in
#     mkFormat for why the earlier free-floating chip row was a misclick trap.
#   - a quiet-hours story, duplicate stacking, and a 20-deep history — the exact
#     window herald-center polls.
#
# dunst's only real conditional is a rule filter, so the per-urgency treatment
# AND the empty-body treatment are both carried by rules: the special urgency_*
# sections accept colours but not `format`. `body = "^$"` (POSIX regex on) is
# what keeps a body-less sender — volume OSDs, bare `notify-send "hi"` — from
# leaving an orphan │ hairline over a blank line where the context tier would
# be. Rule ORDER is load-bearing (dunst applies rules in file order, later wins)
# and Home Manager emits sections in the attrset's own sorted order, so the
# names below are chosen to sort correctly: `herald-critical` before
# `herald-critical-terse`, and `herald-summons` after everything.
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
  # tier fields always carry a value (closed tier, non-null defaults) except
  # `hot`, which is nullOr-falls-back-to-accent by its own documented contract.
  notifBg = hex (if lv.notif.bg != null then lv.notif.bg else lv.palette.bg);
  notifFg = hex (if lv.notif.fg != null then lv.notif.fg else lv.palette.fg);
  notifUrgent = hex (if lv.notif.urgent != null then lv.notif.urgent else lv.palette.urgent);
  accent = hex lv.palette.accent;
  ink = hex lv.palette.fg;
  hot = hex (if lv.palette.hot != null then lv.palette.hot else lv.palette.accent);

  # The herald's signature: RUST, base16 base0F — the one accentSpread slot no
  # dock temple claims. Falls back the same way the stylix facet synthesizes
  # base0F when a song ships no base16 tier: to palette.urgent.
  rust = hex (if lv.base16 != null then lv.base16.base0F else lv.palette.urgent);

  # Ink opacities as Pango #RRGGBBAA — the retired card's withA() steps.
  # 0.95=F2 0.90=E6 0.85=D9 0.80=CC 0.70=B3 0.60=99 0.55=8C 0.45=73 0.30=4D
  mono = "JetBrainsMono Nerd Font";

  # Calligraphy measured against the 336px text band (360 width - 2*10 padding
  # - 2*2 frame) at mono 8.25pt: 48 cells land flush on the right margin.
  rule = lib.concatStrings (lib.genList (_: "─") 48);
  frieze = lib.concatStrings (lib.genList (_: "═") 48);
  # `%a` is variable, so the top frame's tail is fixed: 28 cells keeps a
  # 17-char sender name on one line.
  tabTail = lib.concatStrings (lib.genList (_: "─") 28);
  dashes = n: lib.concatStrings (lib.genList (_: "─") n);
  spaces = n: lib.concatStrings (lib.genList (_: " ") n);

  # One stele per urgency (× body-present, and the summons). `sig` is the
  # signature ink (rust; terracotta while critical — "terracotta throughout"),
  # `title` the tier-2 ink, `word` the bottom-frame label (the urgency word is
  # single-sourced there, as on the card), `kao` the ledger kaomoji, `tag` the
  # entablature's order tag, `actions` turns the closing frame into the
  # summons' approve/deny button frame (and drops `word` from it).
  #
  # Joined with a literal backslash-n: dunst replaces '\n' in format with a
  # real line break. No literal '%' or '&' may appear in the text (format
  # placeholders / Pango entities), and single quotes only — the generated
  # dunstrc wraps the whole value in double quotes.
  mkFormat =
    {
      sig,
      title,
      word,
      kao,
      tag,
      body ? true,
      actions ? false,
    }:
    lib.concatStringsSep "\\n" (
      [
        # entablature — rust crown ❧ (U+2767, the herald's clef stand-in), the
        # ink wordmark at its real letter-spacing, and the order tag pushed
        # right by a measured space run (every operand on this line is static,
        # so the alignment is exact).
        # The crown's face is PINNED: Noto Serif carries no U+2767, so an
        # unpinned span let Pango fall back to FreeSerif and fake-bold it into
        # a blob. Libertine's open fleuron is the crown khoa picked from a
        # rendered lineup of every U+2767 face on the box (2026-08-17), it is
        # the rig's declared primary serif (fonts.nix), and its regular weight
        # IS the picked form — no bold.
        (
          "<span font='Linux Libertine O 15' foreground='${sig}'>❧</span>"
          + "<span font='Noto Serif 9.75' weight='600' letter_spacing='2304' foreground='${notifFg}'> HERALD</span>"
          + "<span font='${mono} 7.5' foreground='${sig}8C'>${spaces (36 - lib.stringLength tag)}[ ${tag} ]</span>"
        )
        # Tuscan frieze — the plain double rule IS the order's whole ornament.
        # One ═ glyph draws both hairlines (and so carries one alpha, where the
        # card drew 0.70 over 0.35).
        "<span font='${mono} 8.25' foreground='${sig}B3'>${frieze}</span>"
        # tier 1: program name in the box-drawing frame label, open on the right
        "<span font='${mono} 8.25' foreground='${sig}F2'>┌─┤ %a ├${tabTail}</span>"
        # the cleave — the entablature/shaft boundary of the stele
        "<span font='${mono} 8.25' foreground='${sig}8C'>${rule}</span>"
        # tier 2: the notification title, bold serif, full ink
        "<span font='Noto Serif Bold 10.5' foreground='${title}'>%s</span>"
      ]
      ++ lib.optional body (
        # tier 3: context — smaller, dimmer (0.85), behind the signature
        # hairline. The card's hairline spanned the whole block; one │ glyph
        # can only tick the first line, and Pango has no hanging indent.
        "<span font='${mono} 8.25' foreground='${sig}73'>│</span>"
        + "<span font='Noto Sans 9' foreground='${notifFg}D9'> %b</span>"
      )
      ++ [
        # the ledger rule, then the ledger line: urgency kaomoji in signature
        # ink, and the gauge tally in gold (empty for any sender with no value)
        "<span font='${mono} 8.25' foreground='${sig}4D'>${rule}</span>"
        (
          "<span font='${mono} 8.25' foreground='${sig}E6'>${kao}</span>"
          + "<span font='${mono} 7.5' foreground='${accent}'>  %p</span>"
        )
        # closing frame — urgency word in the label, gold 𝄂 barline and the rust
        # ┘ corner, spaced apart the way the card spaced them. On the SUMMONS
        # the frame's label slots ARE the buttons (khoa, 2026-08-17): the free-
        # floating chip row invited aiming at `deny` and left-clicking — which
        # APPROVES, since dunst has no per-region hit testing — and cost two
        # rows besides. Each frame label carries its own gesture name, so
        # there is nothing to aim at and nothing to misread: the card's whole
        # face is left = approve / middle = deny, and the frame says exactly
        # that. approve wears the one laurel standout (paletteHot), deny gold;
        # both closes are measured to the standard frame's 42-cell band.
        (
          if actions then
            "<span font='${mono} 8.25' foreground='${notifFg}CC'>└─┤ </span>"
            + "<span font='${mono} 8.25' foreground='${notifFg}99'>left · </span>"
            + "<span font='${mono} 8.25' foreground='${hot}'>approve</span>"
            + "<span font='${mono} 8.25' foreground='${notifFg}CC'> ├─┤ </span>"
            + "<span font='${mono} 8.25' foreground='${notifFg}99'>middle · </span>"
            + "<span font='${mono} 8.25' foreground='${accent}'>deny</span>"
            + "<span font='${mono} 8.25' foreground='${notifFg}CC'> ├</span>"
            + "<span font='${mono} 8.25' foreground='${sig}8C'>─── </span>"
            + "<span font='Noto Music 12' foreground='${accent}'>𝄂</span>"
            + "<span font='${mono} 8.25' foreground='${sig}F2'> ┘</span>"
          else
            "<span font='${mono} 8.25' foreground='${notifFg}CC'>└─┤ ${word} ├</span>"
            + "<span font='${mono} 8.25' foreground='${sig}8C'>${dashes (35 - lib.stringLength word)} </span>"
            + "<span font='Noto Music 12' foreground='${accent}'>𝄂</span>"
            + "<span font='${mono} 8.25' foreground='${sig}F2'> ┘</span>"
        )
      ]
    );

  # Shared per-urgency dress: opaque marble stele, ink text, gilded gauge.
  steleColors = {
    background = notifBg;
    foreground = notifFg;
    highlight = accent; # progress-bar fill — gold, the one gauge ink
  };

  # A stele rule: the shared dress, this urgency's whole treatment, and the
  # `body = "^$"` filter on the terse variant.
  mkStele =
    {
      urgency,
      sig,
      title,
      word,
      kao,
      frame,
      timeout,
      body,
      pauseLevel ? null,
    }:
    steleColors
    // {
      msg_urgency = urgency;
      frame_color = frame;
      inherit timeout;
      format = mkFormat {
        inherit
          sig
          title
          word
          kao
          body
          ;
        tag = "notify";
      };
    }
    // lib.optionalAttrs (!body) { body = "^$"; }
    // lib.optionalAttrs (pauseLevel != null) { override_pause_level = pauseLevel; };

  urgencies = [
    {
      urgency = "low";
      sig = rust;
      title = notifFg;
      word = "low";
      kao = "(´ω｀)"; # at ease, unhurried
      frame = ink;
      timeout = "5s"; # the card's house default when the sender leaves it to us
      pauseLevel = null;
    }
    {
      urgency = "normal";
      sig = rust;
      title = notifFg;
      word = "normal"; # attentive, on the case
      kao = "( ・ω・)ノ";
      frame = ink;
      timeout = "5s";
      pauseLevel = null;
    }
    {
      # critical: terracotta ink throughout, never times out, and loud enough
      # to pierce quiet hours (see the DND note on default_pause_level).
      urgency = "critical";
      sig = notifUrgent;
      title = notifUrgent;
      word = "critical";
      kao = "(ノ｀ｏ´)ノ"; # alarmed — a critical herald
      frame = notifUrgent;
      timeout = 0;
      pauseLevel = 70;
    }
  ];
in
{
  options.aoide.dunst.enable =
    lib.mkEnableOption "dunst notification daemon (the herald's delivery backend)";

  config = lib.mkIf config.aoide.dunst.enable {
    # libnotify rides along for `notify-send`: the stock client the freedesktop
    # world (and aoide's own shellbridge mode-toggle path) reaches for. dunstify
    # alone left it missing on the box.
    environment.systemPackages = [
      pkgs.dunst
      pkgs.libnotify
    ];

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
          # the card's 4px inter-row breath, as leading
          line_height = 3;
          word_wrap = true;

          # Sender text is DATA: escaped and drawn literally, exactly as the
          # retired card's `textFormat: Text.PlainText`. The format's own markup
          # is parsed regardless of this setting — see the header for the
          # measured reason `full` is not an option here.
          markup = "no";

          # ── images: the icon slot, on top, so a raster plate never narrows
          # the calligraphy. Square, and clamped so nothing outgrows the
          # stele it sits on. ───────────────────────────────────────────────
          icon_position = "top";
          min_icon_size = 24;
          max_icon_size = 48;
          icon_corner_radius = 0;
          text_icon_padding = 8;

          # ── measured off, both for the same defect: dunst injects these
          # OUTSIDE the format's markup, in the default font, where they tear
          # the frames open (header has the detail). Stacking stays on; only
          # its numeral goes. ──────────────────────────────────────────────
          show_age_threshold = -1;
          hide_duplicate_count = true;
          stack_duplicates = true;
          # the implicit "default" action is click-anywhere, never chrome — and
          # dunst's indicator is more unstyled text outside the markup
          show_indicators = false;
          # that synthetic card bypasses `format` and cannot be dressed
          indicate_hidden = false;
          # 4 steles ≈ 840px: what actually fits above the bar. The rest waits.
          notification_limit = 4;
          # the exact window herald-center polls out of `dunstctl history`
          history_length = 20;

          # ── quiet hours ──────────────────────────────────────────────────
          # `dunstctl set-paused true` (level 100) is total silence — nothing
          # gets through. `dunstctl set-pause-level 60` is the useful middle:
          # ordinary toasts hold, critical (70) and the permission summons (90)
          # still land. Verified live.
          default_pause_level = 0;

          # ── the gilded gauge (volume/brightness OSDs) — gold fill in a 1px
          # ink-framed trough, square, the full text width ─────────────────
          progress_bar = true;
          progress_bar_height = 8;
          progress_bar_frame_width = 1;
          progress_bar_corner_radius = 0;
          progress_bar_min_width = 336;
          progress_bar_max_width = 336;
          progress_bar_horizontal_alignment = "left";

          # ── the mouse map ────────────────────────────────────────────────
          # left: invoke the card's action, then close — which for an ordinary
          # actionless toast is just the card's own click-anywhere-dismiss, and
          # for a summons is APPROVE. middle: close this one — the summons reads
          # that as DENY (see the herald-summons rule). right: clear the desk.
          # dunst can route only one named action to a button, which is why
          # deny rides the close path rather than a second action.
          mouse_left_click = "do_action, close_current";
          mouse_middle_click = "close_current";
          mouse_right_click = "close_all";

          # `body = "^$"` on the terse rules needs real regex, not globbing.
          enable_posix_regex = true;
        };
      }
      // lib.listToAttrs (
        lib.concatMap (u: [
          {
            name = "herald-${u.urgency}";
            value = mkStele (u // { body = true; });
          }
          {
            name = "herald-${u.urgency}-terse";
            value = mkStele (u // { body = false; });
          }
        ]) urgencies
      )
      // {
        # ── the SUMMONS — an agent asking permission is not a toast ────────
        # Raised only by `aoide graph permit`, matched on its category so any
        # other aoide notification stays an ordinary toast. It is critical
        # urgency (never times out) but overrides the terracotta alarm dress
        # with the house's gold: distinct from both an ordinary card (ink) and
        # something going wrong (terracotta), and neutral — a green frame on a
        # permission gate would lean on the answer. Sorts last on purpose, so
        # it wins over herald-critical.
        # The category string is a CONTRACT with the verb that raises it —
        # `SUMMONS_CATEGORY` in pkgs/aoide/crates/conduct/src/graph/permit.rs.
        # Rename either side alone and the summons silently degrades to a
        # toast. That verb always sends a body (the ask, or at minimum the
        # waiting session's name), which is why there is no -terse twin here.
        herald-summons = steleColors // {
          category = "x-aoide.permission";
          frame_color = accent;
          timeout = 0;
          override_pause_level = 90; # a summons pierces quiet hours
          format = mkFormat {
            sig = rust; # the herald's own signature, not the alarm's
            title = notifFg;
            word = "summons";
            kao = "(ノ｀ｏ´)ノ"; # the alarmed herald: someone is waiting on you
            tag = "summons";
            actions = true;
          };
        };
      };
    };
  };
}
