# The Standard — Design Intent

**Rice:** default (The Standard)
**Palette:** Catppuccin Mocha (base16)
**Cover:** placeholder — see `cover-ref.txt`

---

## Palette Rationale

Catppuccin Mocha was chosen as the Aoide default for three reasons:

1. **Wide ecosystem support.** Catppuccin has maintained base16 mappings, GTK
   themes, terminal schemes, and editor themes. Stylix can consume it natively
   as a base16 scheme, so the fan-out to every nix-managed app is zero-effort.

2. **Legible contrast ratios.** The bg/fg pair (#1e1e2e / #cdd6f4) achieves
   WCAG AA contrast at the bar and notification font sizes. The accent blue
   (#89b4fa) and urgent red (#f38ba8) are clearly distinguishable under most
   display profiles.

3. **Agent reasoning.** Catppuccin Mocha is well-represented in Melete's
   training corpus. When `rice gen` starts from this baseline, the agent can
   reason about transpositions and complements by name rather than by raw
   hex arithmetic.

## Component Tier

All component overrides are null in the default rice — every surface falls
back to the palette. This gives the cleanest baseline for `rice gen`: there
are no hidden overrides to undo before a transposition.

## Geometry

v0 carries no geometry-tier notes. The compositor facet uses these defaults
(baked into `hyprNoteConfig` in `modules/facets/compositor/default.nix`):

| Property     | Value |
|---|---|
| gaps_out     | 8 px  |
| gaps_in      | 6 px  |
| border_size  | 2 px  |
| rounding     | 8 px  |
| blur         | enabled, size 8, passes 3 |

When a geometry tier is added in v1, these become note reads.

## Iteration Log

- 2026-07-26: initial shipped default; palette + component stubs; no cover yet.
- 2026-07-26: gadget dock (agentWidgets) — Win7-sidebar homage: right-edge column of gadgets (TERMINALS/DAG/CLOCK/METERS) in ASCII/box-drawing chrome (╔═[ TITLE ]═╗, ├─ └─ tree limbs, [▓▓▓░░░] gauges); glass = translucent paletteBg (opacity ~0.72) over the compositor's Hyprland blur; all colors from notes; geometry stays v0 defaults (dock width/margins local until a geometry note tier lands).

## Notes for Melete

- `rice gen` without a prompt starts from this rice plus the covers/ library.
- To transpose: `aoide rice transpose default <key-name>` applies a different
  palette while keeping component null (clean slate).
- Liner notes here are ingested before every `gen` run; append observations
  after adopt/reject decisions.
