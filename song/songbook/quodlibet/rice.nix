# song/songbook/quodlibet/rice.nix — "quodlibet", the first real borrow.
#
# A quodlibet is the musical form defined by borrowing: a piece assembled
# from other people's existing tunes sounded together (Bach, Goldberg
# var. 30). This song owns no widget bodies at all — it composes 12 slots
# from sonata and 2 (`bar`, `herald`) from fugue, and contributes only a
# palette and this file. See design/intent.md for the full rationale.
#
# HOST-AGNOSTIC DISCIPLINE (CONTRACTS.md §5): a song sets ONLY aoide.livery.
# All livery values are literal nix expressions (no song/ runtime reads).
{ lib, config, ... }:
let
  song = import ../../../lib/song.nix { inherit lib; };

  # `_widgets/default.nix` here is NOT a shelf roll-up (contrast sonata's and
  # fugue's, which `readDir` their own sibling `<slot>.nix` files) — it is the
  # composition itself: `sonataWidgets // { inherit (fugueWidgets) bar herald; }`,
  # the documented borrow idiom from `lib/song.nix`'s header. `composeSong`
  # validates the result exactly as it would a self-owned shelf; every record
  # it returns keeps whichever `owner` its authoring song's roll-up bound
  # (`fugue` for `bar`/`herald`, `sonata` for the rest) — quodlibet mints no
  # `owner = "quodlibet"` record anywhere.
  widgets = import ./_widgets { inherit lib; };
in
{
  # Guard: apply only when this host performs "quodlibet".
  config = lib.mkIf (config.aoide.song == "quodlibet") {

    aoide.arrangement.widgets = (song.composeSong widgets).arrangement.widgets;

    # ── Palette tier — mid imperial violet, deliberately between the two
    # parents' extreme polarities (sonata L* 92.5 pale marble, fugue L* 4.6
    # near-black graphite). See design/intent.md for the luminance rationale.
    aoide.livery.palette = {
      bg = "#5f4694"; # mid imperial violet, L* ~36           (base00)
      fg = "#f6f0ff"; # near-white lavender ink               (base05)
      accent = "#ff6fc8"; # hot magenta — the chrome            (base0E)
      urgent = "#ff4d6d"; # rose-red                            (base08)
      hot = "#4dffa0"; # mint — the one-hot trace               (base0B)
    };

    # ── Base16 tier — full sixteen, closed and all-or-nothing ───────────────
    # base03/base04 step UP from the ground (lighter), not down: on a mid
    # ground a "muted" tone has to lighten to stay legible, unlike sonata's
    # and fugue's ramps, which both step away from an extreme.
    aoide.livery.base16 = {
      base00 = "#5f4694"; # ground — mid imperial violet (bg)
      base01 = "#52397f"; # panel — one step below ground
      base02 = "#452e6b"; # hairline/selection — two steps below ground;
      # fugue's Cell.qml paints its hairline from windowBorderInactive, which
      # this song pins to base02 (see window.borderInactive below) — same
      # relationship fugue's own rice.nix keeps, new key.
      base03 = "#b3a3d6"; # muted (comments) — steps UP, stays legible
      base04 = "#d3c8ea"; # caption — steps UP further
      base05 = "#f6f0ff"; # default fg — near-white lavender (fg)
      base06 = "#fbf8ff"; # brighter fg
      base07 = "#fffdff"; # brightest — near-white
      base08 = "#ff4d6d"; # red    — rose (urgent)
      base09 = "#ff9440"; # orange — amber
      base0A = "#ffe14d"; # yellow — caution
      base0B = "#4dffa0"; # green  — mint (hot)
      base0C = "#66eaff"; # cyan   — sky
      base0D = "#8fb4ff"; # blue   — periwinkle (preview/info)
      base0E = "#ff6fc8"; # magenta — hot magenta (= accent)
      base0F = "#d6a3ff"; # violet — pale amethyst
    };

    # ── Component tier (v0) ──────────────────────────────────────────────────
    # null → fall back to palette; the key does the work.
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
    # Active hairline = accent magenta (the "live" chrome); inactive recedes
    # to base02, one step below the ground — the exact relationship fugue's
    # own Cell.qml relies on (it paints from windowBorderInactive), preserved
    # here so a borrowed fugue cell's hairline still recedes instead of
    # growing a loud border.
    aoide.livery.window = {
      border = "#ff6fc8"; # accent
      borderInactive = "#452e6b"; # base02
    };

    # ── Cover-art note ────────────────────────────────────────────────────────
    # null → the stylix facet bakes a deterministic violet solid from
    # palette.bg, same mechanism sonata and fugue use.
    aoide.livery.wallpaper = null;
  };
}
