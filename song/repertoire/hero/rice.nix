# song/repertoire/hero/rice.nix — the "hero" song (the cover's own key).
#
# The palette is drawn from the hero cover itself (song/covers/hero.webp —
# the pianist over mirror water at dusk): deep plum sky for the base, pale
# rose-cream cloudlight for text, dusk rose for the accent, sunset ember for
# urgent. The song that matches its wallpaper.
#
# HOST-AGNOSTIC DISCIPLINE (CONTRACTS.md §5): a song sets ONLY aoide.notes.
# All note values are literal nix expressions (no song/ runtime reads).
{ lib, config, ... }:
{
  # Guard: apply only when this host performs "hero".
  config = lib.mkIf (config.aoide.song == "hero") {

    # ── Palette tier (base16 mapping — the cover's dusk) ───────────────────
    aoide.notes.palette = {
      bg = "#1a1322"; # deep dusk plum        (base00)
      fg = "#ecdfda"; # pale rose-cream light (base05)
      accent = "#d98a96"; # dusk rose          (base0D)
      urgent = "#ff7a68"; # sunset ember       (base08)
    };

    # ── Component tier (v0) ────────────────────────────────────────────────
    # null → fall back to palette; the key does the work (moonlight's idiom).
    aoide.notes.bar = {
      bg = null;
      fg = null;
      accent = null;
    };
    aoide.notes.notif = {
      bg = null;
      fg = null;
      urgent = null;
    };
    aoide.notes.window = {
      border = null;
      borderInactive = null;
    };

    # ── Cover-art note ─────────────────────────────────────────────────────
    # The wallpaper this song IS the palette of. Committed score content
    # (song/covers/ — never a runtime infix), same as the default rice.
    aoide.notes.wallpaper = ../../covers/hero.webp;
  };
}
