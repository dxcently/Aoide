# sonata — Design Intent

**Song:** sonata (committed rice; the selected key on yomi-strix)
**Palette:** a light dusk key — pale peach-cream ground, plum-ink text, dusk
slate-blue accent, crimson-ember urgent, a horizon-green one-hot blaze
**Cover:** `song/covers/yuki-sonata.png` — a pianist at a grand piano on
mirror-still water at dusk

---

## Why this song exists

sonata is the light key currently performed on yomi-strix. Any host in the
fleet performs it with one line — `aoide.song = "sonata";` — and the whole
`aoide.drachma` fan-out swaps with zero other edits.

## Host-agnostic by construction

This song sets ONLY `aoide.drachma`. It names no host, enables no facet or
dendrite, touches no hardware or service, and does not key `stylix.polarity`
(the stylix facet pins polarity light — a song cannot flip it). The venue
(host) decides its instruments; sonata carries only the notes. That is exactly
what lets one score be performed on any host with its own specifics and its own
enabled facet/dendrite set (CONTRACTS.md §5).

## The key — keyed from the cover, region by region

The palette is derived from `yuki-sonata.png` itself. The image is a high-variance
dusk scene: a near-black grand piano and pianist on a mirror-still sheet of
water that doubles the sky, under a sunset cloudbank that runs from
peach-cream light on the upper left to plum-mauve dusk on the upper right, with
an ember horizon band low on the right.

Because the desktop is a **cream frosted-glass** surface — "brightness is
opacity, not colour": kitty renders at `background_opacity` 0.86 and the bar
panes at ~0.58 opacity **over** this ground — `base00` stays in the pale
cream-glass family. Its temperature is shifted from the previous parchment
key toward this image's light register: the peach-cream cloudlight and the
rose mirror-water, rather than a warm classical parchment.

The polarity is **light** (the stylix facet pins it): `base00` is the lightest
value and `base05`–`base07` are the dark inks.

| Region of the cover | Slot(s) | Colour |
|---|---|---|
| pale peach-cream sunlit cloudbank (upper left) | `base00` / `bg` | `#f4e9e2` |
| rose-cream lit cloud mist | `base01` | `#ecdcd8` |
| lavender-rose cloud / pale mirror-water | `base02` | `#dcc7c8` |
| dusk mauve-grey cloud shadow | `base03` | `#b49aa6` |
| dusk slate-plum midtone | `base04` | `#806b7a` |
| the piano's near-black, read as a plum ink | `base05` / `fg` | `#3b2f3a` |
| the piano's lacquer (deeper ink) | `base06` | `#2a212a` |
| the piano's darkest black | `base07` | `#191319` |
| crimson piano-stool cushion / ember red | `base08` / `urgent` | `#b34a52` |
| the ember horizon sunset band | `base09` | `#c96a3c` |
| sunlit cloud-gold highlight | `base0A` | `#cc9a52` |
| the horizon's green transition band | `base0B` / `hot` | `#5f8a7a` |
| cool upper-sky cyan-teal | `base0C` | `#4f8598` |
| dusk slate-blue sky (upper right) | `base0D` / `accent` | `#5a6f9c` |
| dusk plum-mauve cloud (upper right) | `base0E` | `#8a5f88` |
| warm rust (deep cloud shadow / stool frame) | `base0F` | `#9a5b4a` |

The slate-blue is the chrome **accent**; the crimson cushion is **urgent**; the
horizon-green is the single one-hot **trace** colour (`palette.hot`, `base0B`) —
the one blaze the Pantheon grammar reserves for the live/traced element.

## Window frames

Active border is `base0C` cool-sky cyan-teal — the same wireframe hairline the
bar's panes wear; the inactive border recedes to `base01` rose-cream, the light
ground, so an unfocused frame melts back into the desktop.

## Component tier

`bar` and `notif` are all-null (palette everywhere) — the key does the work,
no hidden overrides to undo before a transposition. Only `window` carries the
song-owned border notes above.

## Contrast (light polarity, computed — not eyeballed)

WCAG relative-luminance ratios against `base00` (`#f4e9e2`):

| Pair | Ratio | Requirement | Result |
|---|---|---|---|
| `base05` on `base00` | 10.64:1 | ≥ 4.5:1 (AA body text) | PASS |
| `base04` on `base00` | 4.09:1 | ≥ 3:1 (dim fg) | PASS |
| accent `base0D` on `base00` | 4.20:1 | ≥ 3:1 | PASS |
| urgent `base08` on `base00` | 4.38:1 | ≥ 3:1 | PASS |

## Iteration Log

- 2026-07-26: initial commit as the replay fixture; palette-only, no cover.
- 2026-07-29: re-keyed from `song/covers/sonata.webp` (the dusk pianist). The
  earlier indigo-nocturne intent and the interim warm-parchment (Alma-Tadema
  *Unconscious Rivals*) key are both retired; the light key now reads from the
  cover it actually ships with, region by region, contrast computed above.
- 2026-07-29: cover swapped to `song/covers/yuki-sonata.png` — the same
  artwork as `sonata.webp` (identical scene and dimensions, PNG master), so
  the region-keyed palette carries over unchanged.
