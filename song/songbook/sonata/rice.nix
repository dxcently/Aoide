# song/songbook/sonata/rice.nix — the "sonata" song (the Greek key).
#
# sonata is re-keyed to a GREEK register: a light MARBLE ground, plum/charcoal
# ink kept dark (Stylix pins polarity light — the song cannot flip it), a deep
# ATTIC-GOLD chrome accent, a TERRACOTTA urgent, and a true LAUREL leaf-green
# one-hot trace — the single blaze the grammar reserves for the live/traced
# element. Aegean blue steps back to a preview/info role. The look is a temple
# in daylight: pale stone, dark ink, gold-and-terracotta chrome. There is no
# cover — the wallpaper note is null, so the stylix facet bakes a deterministic
# bright-marble solid from palette.bg (the default song's mechanism). The
# typographic-Greek house grammar this key wears is in design/greek-grammar.md
# (sonata's own grammar, a deliberate divergence from the default song's
# Pantheon wireframe grammar).
#
# HOST-AGNOSTIC DISCIPLINE (CONTRACTS.md §5): a song sets ONLY aoide.livery.
# All livery values are literal nix expressions (no song/ runtime reads).
{ lib, config, ... }:
let
  song = import ../../../lib/song.nix { inherit lib; };

  # `_widgets/` is the widget-record shelf (lib/song.nix's header) — one
  # plain function per slot, rolled up and bound to owner "sonata" by
  # `_widgets/default.nix`. `composeSong` validates every record and folds
  # them into `arrangement.widgets` (empty today: all 14 records leave `kind`
  # unset, so none is a declared registry entry — see each file for why).
  widgets = import ./_widgets { inherit lib; };
in
{
  # Guard: apply only when this host performs "sonata".
  config = lib.mkIf (config.aoide.song == "sonata") {

    aoide.arrangement.widgets = (song.composeSong widgets).arrangement.widgets;

    # ── Palette tier (base16 mapping — the Greek marble register) ───────────
    aoide.livery.palette = {
      bg = "#f2ebde"; # pale warm marble ground            (base00)
      fg = "#2f2a33"; # plum-charcoal ink                  (base05)
      accent = "#a07414"; # deep Attic GOLD — the chrome accent (base0A)
      urgent = "#b0472f"; # terracotta                     (base08)
      # The one-hot trace colour — TRUE laurel leaf-green, the single blaze the
      # grammar reserves for the hot/traced element (matches base0B below). Gold
      # is now the chrome accent; THIS green blazes on the one traced row (the
      # DAG/TERMINALS trace link) — kept 71° off gold so it never muddies. The
      # aegean blue (base0D) steps back to a PREVIEW/INFO role (workspace preview
      # ring, links), NOT active-state chrome. null → accent.
      hot = "#4e8b45"; # true laurel leaf-green            (base0B)
    };

    # ── Base16 tier — the Greek marble palette ──────────────────────────────
    # Keyed region-by-region in the Greek key: a warm marble ramp (00–07, from
    # pale sunlit stone down to a plum-charcoal ink) with an accent set drawn
    # from a temple's materials — terracotta clay, clay-orange, Attic gold,
    # laurel green, bronze-verdigris, aegean blue, Tyrian/amethyst purple, and
    # a deep bronze. Slots follow the base16 standard.
    aoide.livery.base16 = {
      base00 = "#f2ebde"; # lightest bg — pale warm marble (sunlit stone)
      base01 = "#e8dfcc"; # lighter bg (status) — marble in soft shade
      base02 = "#dacdb2"; # selection — aged/weathered marble
      base03 = "#a99a82"; # comments — weathered-stone grey-tan (recedes)
      base04 = "#6d6250"; # dark fg — stone-slate midtone
      base05 = "#2f2a33"; # default fg — plum-charcoal ink
      base06 = "#211d26"; # light fg (deeper ink) — obsidian plum
      base07 = "#14111a"; # brightest — near-black ink
      base08 = "#b0472f"; # red    — terracotta (urgent)
      base09 = "#c06a35"; # orange — clay / kiln-fired orange
      base0A = "#a07414"; # yellow — deep Attic gold (unified with the chrome accent)
      base0B = "#4e8b45"; # green  — true laurel leaf-green (one-hot trace)
      base0C = "#3f867e"; # cyan   — bronze verdigris (structural outlines)
      base0D = "#345f81"; # blue   — aegean deep-blue (preview/info role, not chrome accent)
      base0E = "#6f4373"; # magenta— Tyrian / murex purple (project volumes)
      base0F = "#8a5a34"; # brown  — deep bronze / clay
    };

    # ── Component tier (v0) ────────────────────────────────────────────────
    # null → fall back to palette; the key does the work (no hidden overrides
    # to undo before a transposition).
    aoide.livery.bar = {
      bg = null;
      fg = null;
      accent = null;
    };
    aoide.livery.notif = {
      bg = null;
      fg = null;
      urgent = null;
    };
    # Window frames: the active hairline is base0A Attic gold — the same
    # chrome accent the bar and gadget wireframe outlines wear — so the
    # focused window reads as the "live" one. The inactive frame steps back
    # to base0C bronze-verdigris, a cool blue-teal that recedes without
    # vanishing into the ground. The window key stays a component-tier note
    # the song owns.
    aoide.livery.window = {
      border = "#a07414"; # base0A Attic gold — active (= palette.accent)
      borderInactive = "#3f867e"; # base0C bronze-verdigris — inactive (cool teal)
    };

    # ── Cover-art note ─────────────────────────────────────────────────────
    # null → the stylix facet bakes a DETERMINISTIC bright-marble solid from
    # palette.bg (#f2ebde) — the same null-wallpaper-to-solid mechanism the
    # stylix facet applies to any song with no cover note. The retired
    # covers/yuki-sonata.png reference is dropped: sonata's colours fit the
    # Greek theme, not a photograph (khoa). The wallpaper switcher handles photos
    # live; the song's DEFAULT ground is a clean bright marble field, which keeps
    # the light-polarity contrast valid under the now-more-transparent bar.
    aoide.livery.wallpaper = null;
  };
}
