# song/songbook/nocturne/rice.nix — "nocturne": a night-piece demo proving (a)
# the arrangement registry's `kind = "dock"` widget type live (only unit-
# tested/no-op-verified before this — Phase 12) and (b) baseline-fallback
# structural inheritance (CONTRACTS.md §5): nocturne authors NO bar.qml or
# herald-center.qml of its own — StagingEngine.resolveSong falls those slots
# through to sonata's OWN committed widget bodies wholesale (identical
# structure/layout/text, only the palette differs). Scaffolded via `aoide
# rice compose nocturne --from sonata`, then hand-edited: `rice compose` has
# no template for `aoide.arrangement` (same gap etude's own header notes) —
# this block is authored directly, matching the shape `rice lint`
# (`livery/schema.rs`) validates. Name deliberately avoids "moonlight-sonata"
# — that name is reserved for a separate, real future design-song; nocturne
# is a throwaway demo only.
#
# HOST-AGNOSTIC DISCIPLINE (CONTRACTS.md §5): a song sets ONLY aoide.livery
# and aoide.arrangement. All values are literal nix (no song/ runtime reads).
{ lib, config, ... }:
{
  config = lib.mkIf (config.aoide.song == "nocturne") {

    # ── Palette tier — deep navy/indigo night, pale moonlight-silver accent ──
    # Deliberately dark and distinct from BOTH sonata's light marble register
    # and etude's violet demo palette, so the two throwaway songs never read
    # as the same thing if khoa flips between them later.
    aoide.livery.palette = {
      bg = "#0a0e27"; # deep navy/indigo night ground
      fg = "#e8ecf7"; # near-white cool ivory ink — high contrast on navy
      accent = "#9fb8e0"; # pale moonlight-silver-blue chrome
      urgent = "#e2607a"; # rose-red, readable against navy
    };

    # Window frames follow the palette (border = accent, borderInactive = bg)
    # — written explicit here so the resolved value is self-evident, same
    # style sonata's own rice.nix uses.
    aoide.livery.window = {
      border = "#9fb8e0";
      borderInactive = "#0a0e27";
    };

    # ── Arrangement — declares ONE brand-new dock-kind widget type ──────────
    # Phase 12's actual technical proof: the "dock" kind mounts as an Item
    # into AoidePanel's gadget column (SongGadgets.qml), alongside the six
    # shipped gadgets and herald-center. `order = 0` is the only sort key
    # among declared dock entries; the widget body lives at
    # song/songbook/nocturne/widgets/vigil.qml, same convention as any slot.
    aoide.arrangement.widgets.vigil = {
      kind = "dock";
      order = 0;
    };
  };
}
