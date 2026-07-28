---
type: concept
created: 2026-07-25
updated: 2026-07-28
tags: [aoide, notes, theming]
source: "[[references/AOIDE-HANDOFF]]"
---

# Notes — the Seam Between Score and Performance

This seam is where the frozen nix layer and the live desktop meet: values frozen into the crystal, sounded at runtime. They are Aoide's design-token layer — the container stays W3C design-tokens format. **`drachma` is the canonical name**: "notes" and "drachma" are one thing, not a values-vs-engine split — the tokens ARE drachma, named for the coin the mint stamps, and the same word names the package that resolves/lints/emits them. "Notes" survives only as the musical image (Aoide = song); the option is `aoide.drachma`, the runtime file is `stage/drachma.json`, and the package ([[drachma]]) is a standalone Node package with no external runtime dependencies.

## Note Package Contents

The note package (`aoide.drachma` nix option) contains:

- **Resolver** — tiered reference resolution (palette → semantic → component).
- **Schema lint** — validates note files; `rice lint` calls this before any preview.
- **Live-side emitters** — three targets:
  - `stage/drachma.json` for [[Quickshell]] (QML reads this file; hot-reload at rehearsal).
  - `hyprctl` dispatcher for compositor properties.
  - Terminal OSC sequences for color scheme injection.

Every facet consumes notes and nothing else. No module reads another module. The coupling discipline is contractual, not polite.

## Two Fan-Outs, One Source

```
notes (single source)
    ├── stage/drachma.json  →  Quickshell + hyprctl + terminal OSC  (rehearsal / live)
    └── rice.nix → Stylix  →  every nix-manageable app             (recording / adopted)
```

[[Stylix]] is the baked fan-out. `rice.nix` feeds one base16 scheme into Stylix (plus fonts, cursor, wallpaper) and Stylix themes every nix-manageable target — GTK/Qt, terminal, editors, browser, boot. The note package keeps only the live side.

Because both fan-outs derive from the same note values, preview state and adopted state cannot diverge. This is the "zero drift" guarantee.

## Tier Structure

| Tier | Status | Notes |
|---|---|---|
| Palette (base16) | Settled | [[Stylix]] consumes natively; no open questions |
| Semantic tier | Open (v1 design-system work) | Names meanings, survives transposition |
| Component tier | Open (v1 design-system work) | Maps semantics to specific surfaces |

The open schema question is scoped to the semantic and component tiers only. The palette tier is closed.

## Prior Art — Wrap, Don't Rewrite

Style Dictionary and the W3C design-tokens format already provide tiered reference resolution and multi-format emission. The engine wraps these rather than reimplementing a resolver. Notes are Aoide's name for the design-token layer; the container is W3C design-tokens. The genuinely missing pieces are the Aoide-specific emitters (QML/stage, hyprctl, OSC).

## Provisional v0 Schema

Until the design-system v1 lands, the engine builds against a provisional drachma schema v0 inside the W3C design-tokens container:

- Palette: `bg`, `fg`, `accent`, `urgent` (base16 values).
- Component overrides: `bar.*`, `notif.*`, `window.*`.

The update playbook migrates songbook modules from v0 to v1 when v1 supersedes. Building against no schema at all was rejected.

## Stylix Overlap Resolution

The [[Quickshell]] facet declares the surfaces it owns; [[Stylix]] disables derivation for those surfaces from that declaration. The flake's `checks` assert that no surface has two owners. They fail eval if any module reads `song/` runtime paths at build time — `stage/` can never become load-bearing for the nix build.

## Related

- [[Self-Ricing]]
- [[Song-Vocabulary]]
- [[Stylix]]
- [[Snowflake-Anatomy]]
- [[drachma]]
- [[Codebase]]
