# modules/rime/default/rice.nix — the shipped default rice (The Standard).
#
# This is the immutable baseline rice shipped with Aoide.
# `rice gen` starts from this unless told otherwise. The baseline is always
# defined, never guessed (concepts/Self-Ricing).
#
# Rules enforced by checks.no-song-read (CONTRACTS.md §4):
#   - NEVER read song/ runtime paths at build time.
#   - stage/ is live state (gitignored); the nix build must not depend on it.
#   - All note values are literal nix expressions, not file reads.
#
# This module is discovered by the walker (lib/walk.nix) and applies when
# `aoide.song == "default"` — i.e. when the host performs the standard. It is
# song "default": the guaranteed-present baseline any host gets when it names
# no other song. Committed songs live under song/repertoire/<name>/rice.nix and
# guard the same way on `aoide.song == "<name>"` (CONTRACTS.md §5); naming one
# in a host swaps this whole notes fan-out with zero other edits.
#
# The base16 scheme used here is Catppuccin Mocha — chosen as the Aoide
# default for its wide ecosystem support, legible contrast ratios, and
# established community tooling (Melete can reason about it by name). The
# cover-art note points at the shipped hero wallpaper (a literal store path).
{ lib, config, ... }:
{
  # Only apply when this host performs song "default". The standard is the
  # guaranteed baseline — a host that names no song performs it. A song sets
  # ONLY aoide.notes; never host options or facet/dendrite enables.
  config = lib.mkIf (config.aoide.song == "default") {

    # ── Palette tier (base16 Catppuccin Mocha) ─────────────────────────────
    # base00 → bg, base05 → fg, base0D → accent, base08 → urgent.
    # These are the defaults declared in options.nix; setting them here makes
    # the intent explicit and gives `rice gen` a concrete starting point.
    aoide.notes.palette = {
      bg = "#1e1e2e"; # Catppuccin Mocha base (base00)
      fg = "#cdd6f4"; # Catppuccin Mocha text (base05)
      accent = "#89b4fa"; # Catppuccin Mocha blue (base0D)
      urgent = "#f38ba8"; # Catppuccin Mocha red  (base08)
    };

    # ── Component tier (v0 overrides) ──────────────────────────────────────
    # null means "fall back to palette" — the facets apply the fallback.
    # The default rice uses palette values everywhere (no component overrides),
    # which gives the cleanest baseline for `rice gen` to start from.
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
    # A literal nix path (copied to the store — not a song/ runtime read), so
    # the facet bakes it as the Stylix base-context image instead of the
    # solid-colour fallback. null here would take that fallback.
    aoide.notes.wallpaper = ../../../assets/wallpapers/hero.webp;
  };
}
