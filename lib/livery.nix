# lib/livery.nix — the venue-recolour resolver (CONTRACTS.md §1, override tier).
#
# `aoide.livery.override.*` is a HOST-set, never song-set tier. Two registers,
# two passes, fixed order:
#
#   1. Anchor pass (`anchorResolve`, Phase 1, unchanged) — the five optional
#      palette-anchor hexes (`override.{bg,fg,accent,urgent,hot}`). A
#      RECOLOUR, not a re-key: for each overridden anchor, every livery
#      colour equal to the song's AUTHORED value for that anchor — palette,
#      base16, component tiers — is rewritten to the override value, computed
#      in one simultaneous pass against the authored values (so two overrides
#      can never chain through each other).
#   2. Named-key overlay pass (Phase 1b) — `override.{base16,bar,notif,
#      window}.<slot>` set that ONE slot exactly: no propagation, no
#      value-match participation. Applied AFTER the anchor pass as a shallow
#      per-tier merge (last write), so a named key wins its own slot over the
#      anchor recolour — the more specific instruction wins.
#
# Both passes are read-side, not a config-side mkForce: the option system
# keeps storing the song's authored values inert, so there is no
# option-system recursion. Both fan-outs (the baked Stylix scheme and the
# song/stage/livery.json seed) apply the identical rule through this one
# file, so they cannot disagree. `resolve` keeps ONE signature across both
# phases — Phase 1's three consumers (stylix, compositor facets) change zero
# lines.
#
# Usage: (import ./lib/livery.nix { inherit lib; }).resolve config.aoide.livery
{ lib }:
rec {
  norm = c: lib.toLower (lib.removePrefix "#" c);

  # authored-anchor-value → override-value, for every set anchor. Computed
  # against AUTHORED palette values only (one simultaneous pass).
  overrideMap =
    livery:
    let
      o = livery.override or { };
      p = livery.palette;
      anchors = [
        "bg"
        "fg"
        "accent"
        "urgent"
        "hot"
      ];
      live = lib.filter (a: (o.${a} or null) != null && (p.${a} or null) != null) anchors;
    in
    lib.listToAttrs (map (a: lib.nameValuePair (norm p.${a}) o.${a}) live);

  # Pass 1 — the anchor recolour (Phase 1, unchanged law).
  anchorResolve =
    livery:
    let
      m = overrideMap livery;
      sub = c: if c == null then null else (m.${norm c} or c);
      subAttrs = lib.mapAttrs (_: sub);
      o = livery.override or { };
    in
    livery
    // {
      palette =
        subAttrs livery.palette
        // lib.optionalAttrs (livery.palette.hot == null && (o.hot or null) != null) {
          hot = o.hot; # null-hot direct set — no authored value to chase
        };
      base16 = if livery.base16 == null then null else subAttrs livery.base16;
      bar = subAttrs livery.bar;
      notif = subAttrs livery.notif;
      window = subAttrs livery.window;
    };

  # Pass 2 — the named-key overlay (Phase 1b). Non-null named overrides win
  # their slot, last-write, over whatever the anchor pass produced.
  resolve =
    livery:
    let
      base = anchorResolve livery;
      o = livery.override or { };
      named = tier: lib.filterAttrs (_: v: v != null) (o.${tier} or { });
      overlay = tier: authored: authored // named tier;
      namedB16 = named "base16";
    in
    assert lib.assertMsg (base.base16 != null || namedB16 == { })
      "aoide.livery.override.base16.${lib.concatStringsSep "/" (lib.attrNames namedB16)}: the performed song carries no base16 scheme (aoide.livery.base16 is null) to patch a named slot into.";
    base
    // {
      base16 = if base.base16 == null then null else base.base16 // namedB16;
      bar = overlay "bar" base.bar;
      notif = overlay "notif" base.notif;
      window = overlay "window" base.window;
    };

  # The non-null named overrides as a tier-shaped attrset — Phase 2's stage-
  # seed jq patch consumes this (`. * $slots`, applied after the value-walk,
  # same pass order as `resolve` so the fan-outs agree on precedence). Empty
  # `{}` when no named override is set. Exported now so the seam is stable;
  # nothing binds it until Phase 2.
  slotPatch =
    livery:
    let
      o = livery.override or { };
      named = tier: lib.filterAttrs (_: v: v != null) (o.${tier} or { });
    in
    lib.filterAttrs (_: v: v != { }) {
      base16 = named "base16";
      bar = named "bar";
      notif = named "notif";
      window = named "window";
    };
}
