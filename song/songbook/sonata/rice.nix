# song/songbook/sonata/rice.nix — the "sonata" song (the cover's own key).
#
# The palette is drawn from the cover itself (song/covers/sonata.webp — a
# pianist at a grand piano on mirror-still water at dusk): a LIGHT dusk key.
# base00 is the pale peach-cream of the sunlit cloudbank; text is the piano's
# warm near-black, read as a plum ink; the accent is the dusk slate-blue of
# the upper sky, urgent is the crimson of the piano-stool cushion, and the
# one-hot trace blazes the green of the horizon's transition band. The song
# that matches its wallpaper.
#
# HOST-AGNOSTIC DISCIPLINE (CONTRACTS.md §5): a song sets ONLY aoide.drachma.
# All drachma values are literal nix expressions (no song/ runtime reads).
{ lib, config, ... }:
{
  # Guard: apply only when this host performs "sonata".
  config = lib.mkIf (config.aoide.song == "sonata") {

    # ── Palette tier (base16 mapping — the dusk cover's light register) ─────
    aoide.drachma.palette = {
      bg = "#f4e9e2"; # pale peach-cream cloudbank (base00)
      fg = "#3b2f3a"; # the piano's near-black, read as plum ink (base05)
      accent = "#5a6f9c"; # dusk slate-blue sky            (base0D)
      urgent = "#b34a52"; # crimson piano-stool cushion    (base08)
      # The one-hot trace colour — the green of the horizon's transition band
      # (matches base0B below). The slate-blue stays the chrome accent; THIS
      # blazes on the single hot/traced element (the DAG/TERMINALS traced
      # row). null → accent.
      hot = "#5f8a7a"; # dusk horizon green                 (base0B)
    };

    # ── Base16 tier — the LIGHT dusk cover palette ──────────────────────────
    # Keyed from sonata.webp, region by region: a warm rose-cream ramp
    # (00–07, from the sunlit cloudbank down to the piano's black) with an
    # accent set drawn from the dusk sky, the ember horizon, and the piano's
    # crimson cushion: crimson-ember, sunset orange, cloud-gold, horizon green,
    # cool-sky cyan, dusk slate-blue, plum-mauve cloud, and warm rust.
    # Slots follow the base16 standard.
    aoide.drachma.base16 = {
      base00 = "#f4e9e2"; # lightest bg — pale peach-cream sunlit cloudbank
      base01 = "#ecdcd8"; # lighter bg (status) — rose-cream lit cloud mist
      base02 = "#dcc7c8"; # selection — lavender-rose cloud / pale mirror-water
      base03 = "#b49aa6"; # comments — dusk mauve-grey cloud shadow
      base04 = "#806b7a"; # dark fg — dusk slate-plum midtone
      base05 = "#3b2f3a"; # default fg — the piano's near-black, read as plum ink
      base06 = "#2a212a"; # light fg (deeper ink) — the piano's lacquer
      base07 = "#191319"; # brightest — the piano's darkest black
      base08 = "#b34a52"; # red    — crimson piano-stool cushion / ember red
      base09 = "#c96a3c"; # orange — the ember horizon sunset band
      base0A = "#cc9a52"; # yellow — sunlit cloud-gold highlight
      base0B = "#5f8a7a"; # green  — dusk horizon green transition band
      base0C = "#4f8598"; # cyan   — cool upper-sky cyan-teal
      base0D = "#5a6f9c"; # blue   — dusk slate-blue sky (upper right)
      base0E = "#8a5f88"; # magenta— dusk plum-mauve cloud (upper right)
      base0F = "#9a5b4a"; # brown  — warm rust (deep cloud shadow / stool frame)
    };

    # ── Component tier (v0) ────────────────────────────────────────────────
    # null → fall back to palette; the key does the work (moonlight's idiom).
    aoide.drachma.bar = {
      bg = null;
      fg = null;
      accent = null;
    };
    aoide.drachma.notif = {
      bg = null;
      fg = null;
      urgent = null;
    };
    # Window frames join the Pantheon: the active hairline is base0C cyan-teal —
    # the same wireframe rule the bar's panes wear — and the inactive frame
    # recedes to base01, the light rose-cream ground. The window key stays a
    # component-tier note the song owns.
    aoide.drachma.window = {
      border = "#4f8598"; # base0C cool-sky cyan-teal — active
      borderInactive = "#ecdcd8"; # base01 rose-cream — inactive (recedes to the ground)
    };

    # ── Cover-art note ─────────────────────────────────────────────────────
    # The main wallpaper: `sonata.webp` — the pianist on mirror water at dusk.
    # The base16 palette above is keyed from this image's LIGHT register (the
    # pale peach-cream cloudlight and rose mirror-water), for colour coherence
    # with the desktop's Stylix light polarity.
    aoide.drachma.wallpaper = ../../covers/sonata.webp;
  };
}
