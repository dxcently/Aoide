# song/repertoire/hero/rice.nix — the "sonata" song (the cover's own key).
#
# The palette is drawn from the hero cover itself (song/covers/hero.webp —
# Alma-Tadema's "Unconscious Rivals"): a LIGHT warm classical academic key —
# cream/parchment for the base, deep umber ink for text, dusty cornflower for
# the accent, muted rose for urgent. The song that matches its wallpaper.
#
# HOST-AGNOSTIC DISCIPLINE (CONTRACTS.md §5): a song sets ONLY aoide.notes.
# All note values are literal nix expressions (no song/ runtime reads).
{ lib, config, ... }:
{
  # Guard: apply only when this host performs "hero".
  config = lib.mkIf (config.aoide.song == "sonata") {

    # ── Palette tier (base16 mapping — the painting's warm cream light) ────
    aoide.notes.palette = {
      bg = "#faf5ec"; # bright cream paper     (base00)
      fg = "#423420"; # deep umber ink        (base05)
      accent = "#4f74a0"; # dusty cornflower   (base0D)
      urgent = "#b0475f"; # muted rose        (base08)
      # The one-hot trace colour — the painting's sage-green accent
      # (matches base0B below). Cornflower stays the chrome accent; THIS
      # blazes on the single hot/traced element (the DAG/TERMINALS traced
      # row). null → accent.
      hot = "#6f8a4f"; # sage green            (base0B)
    };

    # ── Base16 tier — the LIGHT warm painting palette ───────────────────────
    # Keyed from Alma-Tadema's "Unconscious Rivals": a warm cream ramp
    # (00–07, lightest to darkest umber) with an accent set drawn from the
    # painting's terracotta vault, marble, azalea, and sage: muted rose,
    # burnt terracotta, ochre gold, sage green, teal, cornflower blue,
    # dusty plum, and warm brown. Slots follow the base16 standard.
    aoide.notes.base16 = {
      base00 = "#faf5ec"; # lightest bg — bright cream paper (whiter)
      base01 = "#f0e7d5"; # lighter bg (status)
      base02 = "#ddcaa6"; # selection
      base03 = "#b0997a"; # comments
      base04 = "#8a6f50"; # dark fg
      base05 = "#423420"; # default fg — deep umber ink
      base06 = "#2f2416"; # light fg (deeper ink)
      base07 = "#1d160d"; # brightest — the darkest ink
      base08 = "#b0475f"; # red    — muted rose
      base09 = "#c96038"; # orange — burnt terracotta
      base0A = "#b98a34"; # yellow — ochre gold
      base0B = "#6f8a4f"; # green  — sage
      base0C = "#40897a"; # cyan   — teal
      base0D = "#4f74a0"; # blue   — dusty cornflower
      base0E = "#8f5578"; # magenta— dusty plum
      base0F = "#8a5730"; # brown  — warm brown
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
    # Window frames join the Pantheon: the active hairline is base0C teal —
    # the same wireframe rule the bar's panes wear — and the inactive frame
    # recedes to base01, the light ground. The window key stays a
    # component-tier note the song owns.
    aoide.notes.window = {
      border = "#40897a"; # base0C teal — active
      borderInactive = "#8a6f50"; # base04 dark taupe — inactive (the darker frame)
    };

    # ── Cover-art note ─────────────────────────────────────────────────────
    # The main wallpaper: Alma-Tadema's "Unconscious Rivals" — a warm classical
    # academic painting (terracotta vault, marble, azalea, sage). The base16
    # palette above is keyed from this painting's LIGHT warm register, for
    # colour coherence with the desktop's Stylix light polarity.
    aoide.notes.wallpaper = ../../covers/Alma-Tadema_Unconscious_Rivals.jpg;
  };
}
