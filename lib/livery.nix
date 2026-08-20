# lib/livery.nix — the venue-recolour resolver (CONTRACTS.md §1, override tier).
#
# `aoide.livery.override.*` is a HOST-set, never song-set tier (five optional
# palette-anchor hexes). It is a RECOLOUR, not a re-key: for each overridden
# anchor, every livery colour equal to the song's AUTHORED value for that
# anchor — palette, base16, component tiers — is rewritten to the override
# value, computed in one simultaneous pass against the authored values (so two
# overrides can never chain through each other). This is read-side, not a
# config-side mkForce: the option system keeps storing the song's authored
# values inert, so there is no option-system recursion. Both fan-outs (the
# baked Stylix scheme and the song/stage/livery.json seed) apply the identical
# rule through this one file, so they cannot disagree.
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

  # The resolved livery every colour consumer binds instead of the raw option.
  resolve =
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
}
