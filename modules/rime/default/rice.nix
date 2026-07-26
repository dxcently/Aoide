# modules/rime/default/rice.nix — the shipped default rice (The Standard).
#
# This is the immutable baseline rice shipped with Aoide.
# `rice gen` starts from this unless told otherwise. The baseline is always
# defined, never guessed (concepts/Self-Ricing).
#
# Rules enforced by checks.no-song-read (CONTRACTS.md §4):
#   - NEVER read song/ runtime paths at build time.
#   - stage/ is live state (gitignored); the nix build must not depend on it.
#   - All token values are literal nix expressions, not file reads.
#
# This module is discovered by the walker (lib/walk.nix) and applies when
# aoide.enable is true. It sets aoide.tokens to the default rice's palette
# and component overrides. A user's adopted rice overrides these via
# song/repertoire/<name>/rice.nix (which sets the same options with higher
# priority using lib.mkForce or mkOverride).
#
# The base16 scheme used here is Catppuccin Mocha — chosen as the Aoide
# default for its wide ecosystem support, legible contrast ratios, and
# established community tooling (Melete can reason about it by name).
{ lib, config, ... }:
{
  # Only apply when the framework is enabled. The default rice is the
  # unconditional baseline — hosts that want a different default override
  # aoide.tokens in their own rice.nix.
  config = lib.mkIf config.aoide.enable {

    # ── Palette tier (base16 Catppuccin Mocha) ─────────────────────────────
    # base00 → bg, base05 → fg, base0D → accent, base08 → urgent.
    # These are the defaults declared in options.nix; setting them here makes
    # the intent explicit and gives `rice gen` a concrete starting point.
    aoide.tokens.palette = {
      bg     = "#1e1e2e";   # Catppuccin Mocha base (base00)
      fg     = "#cdd6f4";   # Catppuccin Mocha text (base05)
      accent = "#89b4fa";   # Catppuccin Mocha blue (base0D)
      urgent = "#f38ba8";   # Catppuccin Mocha red  (base08)
    };

    # ── Component tier (v0 overrides) ──────────────────────────────────────
    # null means "fall back to palette" — the facets apply the fallback.
    # The default rice uses palette values everywhere (no component overrides),
    # which gives the cleanest baseline for `rice gen` to start from.
    aoide.tokens.bar    = { bg = null; fg = null; accent = null; };
    aoide.tokens.notif  = { bg = null; fg = null; urgent = null; };
    aoide.tokens.window = { border = null; borderInactive = null; };
  };
}
