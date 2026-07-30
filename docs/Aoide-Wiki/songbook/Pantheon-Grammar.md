---
type: design
created: 2026-07-27
updated: 2026-07-29
tags: [aoide, design, rice, song, qml, glyph, pantheon]
source: "[[references/AOIDE-HANDOFF]]"
---

# Pantheon Grammar

**Home: `song/songbook/default/design/pantheon.md`.** The Pantheon grammar is
rice design memory and lives in the songbook — the default rice (The
Standard) owns the house design language; each song's *current*
instantiation of it lives in that song's own `design/intent.md`
(sonata: `song/songbook/sonata/design/intent.md`). This page is the wiki's
pointer to it, kept because the grammar is load-bearing for reading the QML.

What the grammar covers (in full at its home):

- **The depth recipe** — hollow wireframe depth over the Aero-glass base: two
  offset outline copies behind every glass panel; the five tunable constants
  live in `GadgetFrame.qml` (`depthOff1/2`, `depthOpacity1/2`, `depthExtent`).
- **The glyph grammar** — music glyphs carry meaning, never decoration: the
  `♪ 𝄐 𝄽 𝄂 ·` state tier (lockstep with `baton/theme.rs`), workspace tones,
  the kaomoji identity, one seam divider per container, the 𝄞 clef as the
  power button.
- **The role seam** — DrachmaState maps `wireCyan` (base0C), `holoBlue`
  (base0D), `violet` (base0E), `glitchPink` (base08) from the song's base16
  block; colors are roles, and `hot` stays the ONE blaze (the traced/live
  element).
- **The glass** — brightness is opacity, not colour; hard square corners; bar,
  popouts and terminal read as one glass. The alpha values are recorded
  per-song as design memory.

Naming source: the reference stills at `references/pantheon/*.png`.

## Related

- [[songbook/Ricing-Protocol|Ricing Protocol]] — the creation/application
  split and the vision-check the grammar is kept coherent by.
- [[Song-Anatomy]] — the songbook under `song/` where design memory lives.
- [[Self-Ricing]] — the songbook write-back loop (the "self" in self-ricing).
- [[drachma]] — the `aoide.drachma` seam every surface reads its roles from.
- [[Gadget-Dock]] · [[Terminal-Commander]] — the surfaces that wear the grammar.
