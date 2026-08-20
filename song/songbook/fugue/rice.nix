# song/songbook/fugue/rice.nix — "fugue", a flat mono counterpoint to
# sonata's marble.
#
# THE GRAMMAR (four rules; design/intent.md carries the full rationale):
#   1. ASCII only — no Greek, no PUA, no emoji, no box-drawing.
#   2. One typeface: JetBrainsMono Nerd Font, everywhere, no serif.
#   3. Zero radius, zero shadow, zero gradient, zero blur — hard rectangles.
#   4. State is a rule (a 2px underline / a filled block), never a glow;
#      the one transition tier is 120ms linear on colour only.
#
# HOST-AGNOSTIC DISCIPLINE (CONTRACTS.md §5): a song sets ONLY aoide.livery.
# All livery values are literal nix expressions (no song/ runtime reads).
{ lib, config, ... }:
{
  config = lib.mkIf (config.aoide.song == "fugue") {

    # ── Palette tier — graphite/teal/lime, cool and flat ────────────────────
    aoide.livery.palette = {
      bg = "#0d0f12"; # graphite black, cool-cast          (base00)
      fg = "#e4e6eb"; # bone white                         (base05)
      accent = "#3ddbd9"; # signal teal — the chrome        (base0C)
      urgent = "#ff5f56"; # alarm red-orange                (base08)
      hot = "#c6f135"; # acid lime — the one-hot trace       (base0B)
    };

    # ── Base16 tier — full sixteen, closed and all-or-nothing ───────────────
    aoide.livery.base16 = {
      base00 = "#0d0f12"; # graphite black (bg)
      base01 = "#14171c"; # panel
      base02 = "#1e232a"; # selection / hairlines
      base03 = "#414a55"; # muted steel (comments)
      base04 = "#6f7b88"; # caption steel
      base05 = "#e4e6eb"; # bone (fg)
      base06 = "#f0f2f5"; # bright bone
      base07 = "#ffffff"; # white
      base08 = "#ff5f56"; # red    — alarm
      base09 = "#ff9f45"; # orange — amber signal
      base0A = "#ffd23f"; # yellow — caution
      base0B = "#c6f135"; # green  — acid lime (hot)
      base0C = "#3ddbd9"; # cyan   — signal teal (= accent)
      base0D = "#4d9fff"; # blue   — info / preview
      base0E = "#c77dff"; # magenta — violet
      base0F = "#ff7eb6"; # rose
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
    # Active hairline = accent teal (the "live" chrome); inactive recedes to
    # base02 (selection/hairline steel) without vanishing into the graphite.
    aoide.livery.window = {
      border = "#3ddbd9"; # accent
      borderInactive = "#1e232a"; # base02
    };

    # ── Cover-art note ────────────────────────────────────────────────────────
    # null → the stylix facet bakes a deterministic graphite solid from
    # palette.bg, same mechanism sonata uses.
    aoide.livery.wallpaper = null;
  };
}
