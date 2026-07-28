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
      # The one-neon trace colour — the Pantheon stills' optic-nerve green
      # (matches base0B below). Rose stays the chrome accent; THIS blazes on the
      # single hot/traced element (the DAG/TERMINALS traced row). null → accent.
      hot = "#3fe97f"; # optic-nerve neon      (base0B)
    };

    # ── Base16 tier — "pantheon bw" (the terminal scheme) ──────────────────
    # Mainly black & white per khoa: a true grayscale ramp (00–07, faint cool
    # cast so it sits with the plum surfaces) with the accent set drawn from
    # the Pantheon reference stills (~/Aoide-Wiki/references/pantheon/):
    # optic-nerve neon green, wireframe cyan, hologram periwinkle, violet
    # brain-glow, magenta-pink glitch. Slots follow the base16 standard.
    aoide.notes.base16 = {
      base00 = "#0a0a0d"; # near-black field
      base01 = "#141419"; # lighter bg (status)
      base02 = "#26262e"; # selection
      base03 = "#55555f"; # comments
      base04 = "#9a9aa5"; # dark fg
      base05 = "#e8e8ec"; # default fg (near-white)
      base06 = "#f5f5f7"; # light fg
      base07 = "#ffffff"; # brightest — the window-key white
      base08 = "#ff5f87"; # red    — magenta-pink glitch
      base09 = "#d9a066"; # orange — muted amber (kept quiet)
      base0A = "#d8e07a"; # yellow — pale trace-line chartreuse
      base0B = "#3fe97f"; # green  — THE optic-nerve neon
      base0C = "#5fd8e8"; # cyan   — wireframe lines
      base0D = "#7d9bff"; # blue   — hologram periwinkle
      base0E = "#c583f2"; # magenta— violet brain-glow
      base0F = "#9a6b8f"; # brown  — dim mauve (deprecated)
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
    # Window frames join the Pantheon (khoa, round 5): the active hairline is
    # base0C wireCyan — the same wireframe rule the bar's panes wear — and the
    # inactive frame recedes to base01, the dark ground. The dxflake BW pair
    # retires; the window key stays a component-tier note the song owns.
    aoide.notes.window = {
      border = "#5fd8e8"; # base0C wireCyan — active
      borderInactive = "#141419"; # base01 dark ground — inactive
    };

    # ── Cover-art note ─────────────────────────────────────────────────────
    # The main wallpaper: Alma-Tadema's "Unconscious Rivals" — a warm classical
    # academic painting (terracotta vault, marble, azalea, sage). NOTE: the
    # base16 palette above is still hero's dusk-plum, keyed to the OLD cover —
    # re-derive it from this painting for colour coherence (open thread).
    aoide.notes.wallpaper = ../../covers/Alma-Tadema_Unconscious_Rivals.jpg;
  };
}
