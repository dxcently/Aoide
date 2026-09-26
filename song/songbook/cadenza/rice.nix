# song/songbook/cadenza/rice.nix — the "cadenza" song (the phosphor key).
#
# A hacker console: a green-phosphor CRT running a termui dashboard. Near-black
# green-cast ground, a phosphor-green ramp for text, bright phosphor for titles
# and the focused rule, AMBER as the one-hot live element (the classic amber
# tube, kept 105° off green so the two never muddy), and termui's own chart
# accents — red, cyan, magenta — for trouble, information and human words.
# The drawing grammar (panes, instruments, the switchboard, motion, the glow budget)
# is design/intent.md; the per-surface coverage is design/coverage.md.
#
# HOST-AGNOSTIC DISCIPLINE (CONTRACTS.md §5): a song sets ONLY aoide.livery
# and aoide.arrangement. All values are literal nix (no song/ runtime reads).
# The key is DARK. Stylix polarity is not yet a livery field (mkDefault "light"
# in the stylix module); intent.md §5 carries the proposed `livery.polarity`.
{ lib, config, ... }:
{
  config = lib.mkIf (config.aoide.song == "cadenza") {

    # ── Expected paint — the three facet-owned surfaces cadenza keeps mapped ──
    # Same declaration as sonata: bar/wallpaper per output, the dock (the
    # message board) a single surface. On-demand surfaces are not declared.
    aoide.arrangement.surfaces = {
      bar.perMonitor = true;
      wallpaper.perMonitor = true;
      dock.perMonitor = false;
    };

    # ── Palette tier ─────────────────────────────────────────────────────────
    aoide.livery.palette = {
      bg = "#050a06"; # CRT black, green-cast                 (base00)
      fg = "#86f2a0"; # phosphor green — body text            (base05)
      accent = "#39ff6a"; # bright phosphor — titles, focused rule, active pad
      urgent = "#ff4d4d"; # termui red — blocked, failed, summons (base08)
      hot = "#ffb000"; # amber tube — the ONE live element     (base0A)
    };

    # ── Base16 tier — the phosphor ramp + termui accents ─────────────────────
    aoide.livery.base16 = {
      base00 = "#050a06"; # CRT black (bg)
      base01 = "#0a140c"; # pane fill — one step up the tube
      base02 = "#12261a"; # selection / focused row
      base03 = "#4a905d"; # dim phosphor — rules at rest, axes, labels (≥4.5:1 on base00/01)
      base04 = "#62b377"; # mid phosphor — secondary text
      base05 = "#86f2a0"; # phosphor green (fg)
      base06 = "#b6ffc8"; # bright phosphor text
      base07 = "#e4ffea"; # white-hot phosphor
      base08 = "#ff4d4d"; # red     — urgent
      base09 = "#ff7a2e"; # orange  — warning
      base0A = "#ffb000"; # amber   — hot / live
      base0B = "#39ff6a"; # green   — bright phosphor (= accent)
      base0C = "#3fe0d0"; # cyan    — information (termui sparkline)
      base0D = "#4aa8ff"; # blue    — links
      base0E = "#d65cff"; # magenta — human words (termui borderless block)
      base0F = "#8a6a3a"; # brown   — burnt phosphor
    };

    # ── Component tier ───────────────────────────────────────────────────────
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
    aoide.livery.window = {
      border = "#39ff6a"; # bright phosphor — the focused window is the lit one
      borderInactive = "#2e5a3a"; # dim phosphor — recedes without vanishing
    };

    # ── Geometry tier ────────────────────────────────────────────────────────
    # Square, thin, tight, opaque: the CRT is black glass, not frost.
    aoide.livery.geometry = {
      gapsOut = 8;
      gapsIn = 4;
      borderSize = 1;
      rounding = 0;
      blurEnabled = false;
      blurSize = null;
      blurPasses = null;
    };

    # ── Cover-art note ───────────────────────────────────────────────────────
    # null → the stylix facet bakes a solid field from palette.bg: the tube.
    aoide.livery.wallpaper = null;
  };
}
