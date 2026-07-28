---
type: entity
created: 2026-07-25
updated: 2026-07-28
tags: [aoide, theming, base16, stylix, nix]
source: "[[references/AOIDE-HANDOFF]]"
---

# Stylix

Base16-driven whole-system theming; Aoide's "baked fan-out" engine. `rice.nix` feeds a single scheme into Stylix — `base16Scheme`, fonts, cursor, and wallpaper — and Stylix propagates it to every nix-manageable target: terminal, GTK/Qt, icons, cursor, editors, browser, and boot.

Stylix is the recording side of the two-fan-out model. `stage/drachma.json` is the rehearsal side. Both derive from the same drachma source, so preview and adopted state cannot diverge.

Surface ownership is enforced: the Quickshell facet declares which surfaces it owns, and Stylix's options disable derive from that declaration. The flake's `checks` assert that no surface has two owners, catching overlap at eval time rather than at runtime.

GTK/Qt live preview is accepted as adopt-only (app restarts required). A `rice preview --gallery` mode (sample apps restarted under a preview env) is deferred to after v1.

## Implementation

The Stylix facet applies the surface stand-down on **both** module layers — the
NixOS side and the home-manager side — because Stylix splits its targets across
them (mako/dunst/gtklock/hyprlock are HM-side). Each side probes its *own* option
tree and force-disables only the `stylix.targets.<name>` that actually exist
there, so the stand-down is eval-safe across Stylix versions and whichever facet
is walked first. Quickshell-owned surfaces map to concrete Stylix targets only
where one exists: `notifications` → mako/dunst, `lockscreen` → gtklock/hyprlock,
`greeter` → gnome; bar/launcher/osd/agentWidgets have no colliding Stylix target
(pure QML). The `wallpaper` surface is deliberate policy — Stylix's `image` is
always set (as `mkDefault`) so it stays the base-context source and fallback,
while the Quickshell wallpaper layer paints over it live.

The base16 scheme is **synthesised** from the v0 palette (bg→base00, fg→base05,
accent→base0D, urgent→base08, with the resolved component tier informing
surface-adjacent slots) — a provisional derivation. v1's design-system work
replaces this synthesis with a real 16-colour derivation. That said, songs
may also supply the full sixteen slots directly as `aoide.drachma.base16`
(bypassing synthesis) — the `sonata` and `hero` songs both do, keyed by hand
from their wallpapers; see [[design/Ricing-Protocol|the Ricing Protocol]].

## Polarity — light or dark, one switch

`stylix.polarity` (`"light"` or `"dark"`) declares which register the base16
ramp reads as, and every Stylix-managed target honours it. The desktop's
current key is `polarity = "light"` (set `mkDefault` on the Stylix facet):
the `sonata` song's cream/parchment base00 with deep-umber base05 text is a LIGHT
scheme, keyed off the Alma-Tadema *Unconscious Rivals* wallpaper (see
[[design/Pantheon-Grammar|Pantheon Grammar round 5]]). (`hero` is the alternate
dusk-plum key.) Because polarity is a single fan-out switch,
flipping it is cheap — the discipline that makes it *look* right everywhere
(terminal, bar, every widget agreeing) is the vision-check in
[[design/Ricing-Protocol|the Ricing Protocol]], not the switch itself.

## Related

- [[Self-Ricing]]
- [[Quickshell]]
- [[Codebase]]
- [[drachma]]
- [[design/Ricing-Protocol|Ricing Protocol]]
- [[design/Pantheon-Grammar|Pantheon Grammar]]
