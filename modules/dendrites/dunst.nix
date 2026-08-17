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
# Reading order, top down (the plate column runs down the left of the whole
# text block — a sender's image lands in it, and the standing keyline plate
# holds it open when none is sent):
#          ❧ H E R A L D           [ notify ]  entablature: crown, wordmark, tag
#          ╞══════════════════════════════╡    Tuscan frieze, railed
#          ┌─┤ ♪ program ├───                  tier 1: program in a short tab
#     ┌──┐ ├──────────────────────────────┤    the cleave — entablature/shaft
#     │  │ Notification title                  tier 2: bold serif, full ink
#     └──┘ │ context                           tier 3: smaller, dimmer, hairline
#          ├──────────────────────────────┤    the ledger rule
#          ( ・ω・)ノ  [ 60%]                   ledger: kaomoji, gauge tally
#          └─┤ normal ├──────────────── 𝄁 ┘    closing frame: word, barline
#     ▓▓▓▓▓▓▓▓▓▓▓▓░░░░░░░░░░░░░░░░░░░░░░░░░    the plinth: the gilded gauge
#
# Sizes are the retired card's pixel metrics converted at 96dpi (pt = px*0.75,
# and the rig runs scale 1.0): 20px crown → 15pt, 13px wordmark → 9.75pt,
# 11px mono → 8.25pt, 10px mono → 7.5pt, 14px title → 10.5pt, 12px context →
# 9pt. `letter_spacing='2304'` is the card's `letterSpacing: 3` (3px → 2.25pt →
# 2304 Pango units) — a real 1:1 port, not faked with spaces. Vertical rhythm
# is NOT one number: each line carries its own Pango `line_height` factor,
# because dunst's own `line_height` key leads every line alike and a rule row
# and a wrapped paragraph do not want the same breath (khoa asked twice — once
# to tighten it, once to open the margins between the ascii rows back up).
#
# What ports 1:1 — the ❧ crown and the `[ notify ]` order tag, the frieze, both
# box-drawing frames, the cleave, the three text tiers with the │ hairline tick
# on the context, the ledger rule, the per-urgency kaomoji (the card's own
# proven kana vocabulary) and urgency word, the gold barline + rust ┘ closing
# the stele with the same small gap the card left between them, terracotta ink
# throughout while critical, critical never timing out, 2px ink frame, radius 0
# everywhere, and `gap_size` 8 giving every card its own frame — the retired
# 8px popup stack exactly. The width is 340, the pre-widget-slot
# NotificationCard.qml's own number (khoa's directive, over the popup's 360).
# Also 1:1, and the reason the
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
#   - 1px inset rust keyline         -> not on the card (one frame only, kept
#                                       ink — the card's OUTER border), but the
#                                       motif survives at plate scale: the
#                                       standing image plate IS a keyline
#                                       square with a second inset 4px in
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
#                                       288px text band: 41 cells at 8.25pt,
#                                       where one cell is exactly 7px. The
#                                       frieze, both rails and BOTH closing
#                                       frames land on one right edge, to the
#                                       pixel. Overshoot is not a soft failure
#                                       — a run 1px over draws into the padding
#                                       AND Pango allocates a ghost second
#                                       line, which is where the pre-polish
#                                       card's swollen head came from.
#   - a ┐ closing the top frame      -> impossible: `%a` is variable, dunst has
#                                       no padding. So the tail is a SHORT
#                                       3-cell tab that promises no right edge
#                                       at all; the railed rules own the edge,
#                                       and the bottom frame, whose label IS
#                                       static, does close on ┘.
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
#   - REAL images, in a STANDING plate column beside the text (khoa, 2026-08-17:
#     "keep a space next to the text for images and icons", "the image should be
#     integrated inside the elements of the design"). `image-path`, `image-data`
#     and app icons all land in it. The column is 30px + 6px, pinned by
#     min = max so it is the same width whatever arrives — that pin is what
#     lets the calligraphy be measured once (an icon-conditional column would
#     drag every run 36px off its edge on icon-bearing cards alone, measured).
#     A 1920x1080 screenshot lands as a 30x17 plate, an app icon as 30x30: the
#     plate is a mark, not a poster. When no image is sent the slot is not
#     empty — it holds the keyline plate, so the design element is always
#     there and the layout never jumps.
#   - a gilded gauge for progress senders: gold fill in a 1px ink-framed square
#     trough. dunst draws the bar from the card's own padding rather than the
#     text band, so it spans the full 323px inner width — under the plate
#     column too, which is exactly right: it sits BELOW the closing barline and
#     reads as the stele's plinth. `%p` also rides the ledger line in gold and
#     vanishes when the sender set no value.
#   - the SUMMONS: an agent permission prompt is not a toast (see the
#     herald-summons rule). It is the one card whose gestures DO something —
#     left-click approves, middle-click denies, and `aoide graph permit` types
#     the verdict back into the waiting session. Both gestures were driven end
#     to end against a live conducted session before this was written down.
#     The approve/deny controls live IN the closing frame's label slots (each
#     carrying its own gesture name) — see the closing-frame comment in
#     mkFormat for why the earlier free-floating chip row was a misclick trap.
#   - a quiet-hours story, duplicate stacking, and a 20-deep history — the exact
#     window herald-center polls. All four were re-fired and looked at after the
#     shrink: gauge, images, stacking (3 identical → 1 card), pause levels
#     (60 holds a toast, 70 critical and 90 summons still land), history depth.
#   - `idle_threshold` holds a toast's timeout while the chair is empty, and
#     recursive icon-theme lookup resolves an app's own icon on NixOS, where
#     dunst's compiled-in icon_path points at /usr/share dirs that do not exist.
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

  # ── the band, and why every number below is derived from it ──────────────
  # One mono 8.25pt cell is exactly 7px (measured on a live daemon, not
  # assumed: 41 cells = 287px renders, 42 = 294px does not). The text band is
  #   width - 2*frame - 2*horizontal_padding - plateColumn
  #   340    - 2*2     - 2*6                  - (30 + 6)    = 288px
  # so 41 cells (287px) is the flush run and 42 wraps. Overflow is not a soft
  # failure: a run 1px over the band draws INTO the padding and Pango also
  # allocates a phantom second line, which is where the pre-shrink card's
  # airiness came from (three overflowing rules = three ghost lines).
  cells = 41;
  dashes = n: lib.concatStrings (lib.genList (_: "─") n);
  # The full-width runs are RAILED — capped at both ends so they read as
  # members of the frame rather than loose strokes, and so the left edge is
  # marked as deliberately as the right (khoa, 2026-08-17: "decorate it more").
  # Caps cost 2 cells, so the run between them is cells - 2.
  rail = "├${dashes (cells - 2)}┤";
  frieze = "╞${lib.concatStrings (lib.genList (_: "═") (cells - 2))}╡";
  spaces = n: lib.concatStrings (lib.genList (_: " ") n);

  # The closing frames get ONE cell less than the rules (39 + the barline's own
  # ~10px advance + the gap = 287px, landing on the same right edge). `gap` is
  # two U+2009 THIN SPACEs — LITERAL characters below, do not "tidy" them into
  # an ordinary space: a plain space is 7px, which tips the row over the band
  # and wraps the ┘ onto a line of its own (measured, twice).
  closeCells = 39;
  gap = "  ";
  # the barline: U+1D101 DOUBLE BARLINE at Noto Music 10. The card's 𝄂 (FINAL
  # barline, U+1D102) rendered as a gold SLAB at every size on this box — khoa
  # rejected it on sight. 𝄁 is the crisp form AND it rhymes with the frieze's
  # ═, so the stele opens and closes on the same doubled stroke.
  barline = "<span font='Noto Music 10' foreground='${accent}'>𝄁</span>";

  # ── the plate: the image slot, made a standing element of the stele ───────
  # khoa, 2026-08-17: "keep a space next to the text for images and icons",
  # and "the image should be integrated inside the elements of the design".
  # dunst draws the icon in a column beside the text — but ONLY when a sender
  # supplies one, and the text band shrinks by that column, which would drag
  # every measured run 36px off its right edge on icon-bearing cards alone
  # (measured: the runs wrap). So the column is made unconditional: min = max
  # pins it to exactly 30px wide whatever arrives, and every rule carries a
  # `default_icon`, so the band is ONE number and the calligraphy never moves.
  # The default plate is the card's own lost 1px inset keyline, at plate scale
  # — a 30px keyline square with a second inset 4px in, drawn in the card's
  # signature ink. Empty on purpose: it reads as the slot a raster will fill,
  # and when one does (`image-path`, `image-data`, an app icon) it lands in the
  # same 30px column with the same alignment.
  plate =
    name: inkColor:
    pkgs.runCommand "herald-plate-${name}.png" { nativeBuildInputs = [ pkgs.imagemagick ]; } ''
      magick -size 30x30 xc:none -fill none -strokewidth 1 \
        -stroke '${inkColor}' -draw 'rectangle 0.5,0.5 29.5,29.5' \
        -stroke '${inkColor}80' -draw 'rectangle 4.5,4.5 25.5,25.5' $out
    '';

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
        # `line_height` is a Pango span attribute (a FACTOR on the line's
        # natural height, Pango >= 1.50) and it is the only vertical lever
        # dunst leaves: its own `line_height` key adds ONE leading value to
        # every line alike, where a rule row needs almost none and a text row
        # needs all of its own. A single-stroke rule keeps 0.55, the frames
        # 0.80, the three reading tiers their full 1.00.
        (
          "<span line_height='1.05'>"
          + "<span font='Linux Libertine O 15' foreground='${sig}'>❧</span>"
          + "<span font='Noto Serif 9.75' weight='600' letter_spacing='2304' foreground='${notifFg}'> HERALD</span>"
          + "<span font='${mono} 7.5' foreground='${sig}8C'>${spaces (28 - lib.stringLength tag)}[ ${tag} ]</span>"
          + "</span>"
        )
        # Tuscan frieze — the plain double rule IS the order's whole ornament.
        # One ═ glyph draws both hairlines (and so carries one alpha, where the
        # card drew 0.70 over 0.35).
        "<span font='${mono} 8.25' foreground='${sig}B3' line_height='1.0'>${frieze}</span>"
        # tier 1: program name in the frame label. The tail is a SHORT 3-cell
        # tab, not a long run: `%a` is variable, so a long tail would promise a
        # right edge it can only hit for one sender name. Three dashes read as
        # a deliberate tab; the three full-width rules own the right edge.
        "<span font='${mono} 8.25' foreground='${sig}F2' line_height='1.1'>┌─┤ ♪ %a ├───</span>"
        # the cleave — the entablature/shaft boundary of the stele
        "<span font='${mono} 8.25' foreground='${sig}8C' line_height='1.0'>${rail}</span>"
        # tier 2: the notification title, bold serif, full ink
        "<span font='Noto Serif Bold 10.5' foreground='${title}' line_height='1.15'>%s</span>"
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
        "<span font='${mono} 8.25' foreground='${sig}4D' line_height='1.0'>${rail}</span>"
        (
          "<span line_height='1.05'>"
          + "<span font='${mono} 8.25' foreground='${sig}E6'>${kao}</span>"
          + "<span font='${mono} 7.5' foreground='${accent}'>  %p</span>"
          + "</span>"
        )
        # closing frame — urgency word in the label, the gold barline and the
        # rust ┘ corner, spaced apart the way the card spaced them. On the
        # SUMMONS the frame's label slots ARE the buttons (khoa, 2026-08-17):
        # the free-floating chip row invited aiming at `deny` and left-clicking
        # — which APPROVES, since dunst has no per-region hit testing — and
        # cost two rows besides. Each frame label carries its own gesture name,
        # so there is nothing to aim at and nothing to misread: the card's
        # whole face is left = approve / middle = deny, and the frame says
        # exactly that. approve wears the one laurel standout (paletteHot),
        # deny gold. BOTH branches are measured to the same 39 cells: the
        # summons row buys its last cell by sharing ONE ├┤ joint between the
        # two label slots, so it closes on the band edge like every other row.
        (
          "<span line_height='1.1'>"
          + (
            if actions then
              "<span font='${mono} 8.25' foreground='${notifFg}CC'>└─┤ </span>"
              + "<span font='${mono} 8.25' foreground='${notifFg}99'>left · </span>"
              + "<span font='${mono} 8.25' foreground='${hot}'>approve</span>"
              + "<span font='${mono} 8.25' foreground='${notifFg}CC'> ├┤ </span>"
              + "<span font='${mono} 8.25' foreground='${notifFg}99'>middle · </span>"
              + "<span font='${mono} 8.25' foreground='${accent}'>deny</span>"
              + "<span font='${mono} 8.25' foreground='${notifFg}CC'> ├</span>"
              + "<span font='${mono} 8.25' foreground='${sig}8C'>${gap}</span>"
              + barline
              + "<span font='${mono} 8.25' foreground='${sig}F2'> ┘</span>"
            else
              "<span font='${mono} 8.25' foreground='${notifFg}CC'>└─┤ ${word} ├</span>"
              + "<span font='${mono} 8.25' foreground='${sig}8C'>${dashes (
                closeCells - 8 - lib.stringLength word
              )}${gap}</span>"
              + barline
              + "<span font='${mono} 8.25' foreground='${sig}F2'> ┘</span>"
          )
          + "</span>"
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
      # the standing plate, in this urgency's signature ink — a sender's own
      # raster overrides it; what it must never do is go missing, or the band
      # (and every measured run on it) would move. See `plate` above.
      default_icon = plate urgency sig;
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
          # 340, not 360: the pre-widget-slot NotificationCard.qml's own width,
          # and khoa's directive over the retired popup's number (2026-08-17).
          # Every run in the calligraphy is measured to THIS number.
          width = 340;
          height = "(0, 600)";
          origin = "bottom-right";
          offset = "(12, 8)";
          frame_width = 2;
          frame_color = ink;
          gap_size = 8;
          # 6, not the card's 10: the QML card spent its gutter on a 1px inset
          # signature keyline 4px off the border, and dunst draws no second
          # border — so the same 10px here is just empty marble. Tightened on
          # khoa's ask; the calligraphy band above is measured to THIS number.
          padding = 6;
          horizontal_padding = 6;
          font = "Noto Serif 11";
          # 0 here on purpose — the breath is set PER LINE by the format's own
          # `line_height` span factors, because one number cannot serve both a
          # 2px rule and a wrapped body paragraph. dunst's key applies the same
          # leading to every line, which is what left the pre-polish card airy.
          line_height = 0;
          word_wrap = true;

          # Sender text is DATA: escaped and drawn literally, exactly as the
          # retired card's `textFormat: Text.PlainText`. The format's own markup
          # is parsed regardless of this setting — see the header for the
          # measured reason `full` is not an option here.
          markup = "no";

          # ── images: the plate column, BESIDE the text (khoa, 2026-08-17 —
          # an icon on top read as a swollen head). min = max is not a clamp
          # but a PIN: it fixes the column at 30px whatever a sender sends, so
          # the text band is one constant number and every measured run keeps
          # its right edge. Square, of course. ─────────────────────────────
          icon_position = "left";
          min_icon_size = 30;
          max_icon_size = 30;
          icon_corner_radius = 0;
          text_icon_padding = 6;

          # Adwaita is the rig's theme (stylix installs it into the user
          # profile); dunst's compiled-in icon_path points at /usr/share paths
          # that do not exist on NixOS, so recursive theme lookup is what
          # actually resolves an app's own icon from a bare `desktop-entry`.
          enable_recursive_icon_lookup = true;
          icon_theme = "Adwaita";

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
          # away from the keyboard for two minutes: hold the timeout so a
          # toast fired at an empty chair is still there to read on return.
          # (Costs nothing; critical never timed out anyway.)
          idle_threshold = 120;

          # ── the gilded gauge (volume/brightness OSDs) — gold fill in a 1px
          # ink-framed trough, square. dunst draws the bar from the card's own
          # padding, NOT from the text band, so its width is the full inner
          # width (340 - 2*2 frame - 2*6 padding - 1) and it runs under the
          # plate column too: read it as the stele's plinth, which is also
          # where it lands, below the closing barline. ─────────────────────
          progress_bar = true;
          progress_bar_height = 6;
          progress_bar_frame_width = 1;
          progress_bar_corner_radius = 0;
          progress_bar_min_width = 323;
          progress_bar_max_width = 323;
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
          default_icon = plate "summons" accent;
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
