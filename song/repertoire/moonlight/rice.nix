# song/repertoire/moonlight/rice.nix — the "moonlight" song (a committed rice).
#
# The replay fixture. A committed song is host-agnostic: ANY host in the fleet
# performs it by naming it — `aoide.song = "moonlight";` — one line, no other
# edits. This module is walked in by lib/mkHost.nix (like a dendrite) and
# self-gates on `aoide.song == "moonlight"`, so adding a song never touches an
# import list.
#
# HOST-AGNOSTIC DISCIPLINE (CONTRACTS.md §5): a song sets ONLY aoide.notes
# (palette + component tiers) — and, later, cover/chime references inside
# song/. It MUST NEVER set host options (monitors, hardware, services) or
# enable facets/dendrites. The VENUE (host) decides its instruments; the SONG
# carries only the notes. The venue's enabled facet/dendrite set renders these
# notes on its own specifics — that is what makes one score replay anywhere.
#
# All note values are literal nix expressions (no song/ runtime reads), same as
# the shipped standard.
{ lib, config, ... }:
{
  # Guard: apply only when this host performs "moonlight". Named this in a host
  # and the standard stands down, this song's palette fans out instead.
  config = lib.mkIf (config.aoide.song == "moonlight") {

    # ── Palette tier (base16 — a cool nocturne, distinct from the standard) ──
    # A deliberately different key from Catppuccin Mocha so the replay swap is
    # visible at a glance: deeper indigo base, moonlit silver text, cyan accent.
    aoide.notes.palette = {
      bg     = "#0b1021";   # deep midnight indigo (base00)
      fg     = "#c8d3f5";   # moonlit silver text (base05)
      accent = "#82aaff";   # cool moon-cyan accent (base0D)
      urgent = "#ff757f";   # muted rose alarm     (base08)
    };

    # ── Component tier (v0 overrides) ──────────────────────────────────────
    # null means "fall back to palette" — facets apply the fallback. moonlight
    # keeps the arrangement clean (palette everywhere) so the key does the work.
    aoide.notes.bar    = { bg = null; fg = null; accent = null; };
    aoide.notes.notif  = { bg = null; fg = null; urgent = null; };
    aoide.notes.window = { border = null; borderInactive = null; };
  };
}
