# song/songbook/etude/rice.nix — "etude": a throwaway practice piece proving
# the declared widget-type registry pipeline end-to-end (Phase 6 of the
# arrangement-registry feature; not a real dressed song — see
# design/intent.md). Scaffolded via `aoide rice compose etude --from sonata`,
# then hand-edited: `rice compose` has no template for `aoide.arrangement`
# (Phase 3 only shipped `rice lint` validation + hot-sync for it, not a
# compose generator) — this block is authored directly, matching the shape
# `rice lint` (Phase 3, `livery/schema.rs`) validates.
#
# HOST-AGNOSTIC DISCIPLINE (CONTRACTS.md §5): a song sets ONLY aoide.livery
# and aoide.arrangement. All values are literal nix (no song/ runtime reads).
{ lib, config, ... }:
{
  config = lib.mkIf (config.aoide.song == "etude") {

    # ── Palette tier — a trivial, distinct override (not designed) ──────────
    aoide.livery.palette = {
      bg = "#12121c";
      fg = "#e6e6f0";
      accent = "#7c5cff";
      urgent = "#ff5c7c";
    };

    # ── Arrangement — declares one brand-new surface-kind widget type ───────
    # Proves the Phase 1-5 registry pipeline end-to-end: this walks into
    # registry.json (build-time walk + `rice stage` hot-sync) and the
    # compositor facet's layerrules; song/songbook/etude/widgets/demo.qml is
    # the widget body.
    aoide.arrangement.widgets.demo = {
      kind = "surface";
      namespace = "aoide-etude-demo";
      layer = "overlay";
      shortcut = "aoide:etude-demo";
      blur = true;
    };
  };
}
