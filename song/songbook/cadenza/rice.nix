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
# The key is DARK: it declares `aoide.livery.polarity = "dark"` beside its
# palette, so the stylix lane bakes this phosphor ramp as a dark register
# instead of the lane's own light default. Baked only — GTK/Qt take it at
# build time, so a polarity change lands on the user-gated rebuild.
{
  lib,
  config,
  pkgs,
  ...
}:
{
  config = lib.mkIf (config.aoide.song == "cadenza") {

    # ── Expected paint — the three shell-owned surfaces cadenza keeps mapped ──
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

    # ── Polarity — the register the ramp reads as (CONTRACTS.md §1) ──────────
    # Beside the palette, because a palette brings its polarity: a CRT ground
    # is read as DARK, and the stylix lane hands this value to
    # `stylix.polarity` so every Stylix-managed target (GTK/Qt, terminal,
    # editors) reads it as one register. Baked only: no emitter carries it and
    # staging does not apply it, so flipping it is a rebuild, not a stage.
    aoide.livery.polarity = "dark";

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
    # Square, thin, tight, and 0.7 opaque: the CRT is near-black glass, not
    # frost. `terminalOpacity` is the one field here that is not a Hyprland
    # keyword — it rides `song/stage/terminal-colors.conf` as kitty's
    # `background_opacity` (CONTRACTS.md §1), so the phosphor tube stays
    # readable over the wallpaper without going translucent.
    aoide.livery.geometry = {
      gapsOut = 8;
      gapsIn = 4;
      borderSize = 1;
      rounding = 0;
      blurEnabled = false;
      blurSize = null;
      blurPasses = null;
      terminalOpacity = 0.7;
    };

    # ── Font tier ────────────────────────────────────────────────────────────
    # The terminal's face is part of the key. A console sets in a fixed-pitch
    # machine face, not a humanist serif: ShureTechMono (Share Tech Mono,
    # nerd-patched) is squared-off, single-weight and flat — a CRT read, and it
    # carries the box-drawing and block coverage the termui grammar leans on
    # (U+2500/2502/250C/2510, U+2588, U+2591 — checked against the installed
    # face). Widgets keep JetBrainsMono (widgets/Kit.js, intent §2 "one face"):
    # that is the pane's cell, this is the terminal an agent lands in.
    aoide.livery.fonts.monospace = {
      name = "ShureTechMono Nerd Font Mono";
      package = pkgs.nerd-fonts.shure-tech-mono;
    };

    # ── Cover-art note ───────────────────────────────────────────────────────
    # null → the stylix lane bakes a solid field from palette.bg: the tube.
    aoide.livery.wallpaper = null;
  };
}
