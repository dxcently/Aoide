---
type: concept
created: 2026-07-28
updated: 2026-08-25
tags:
  - aoide
  - rice
  - song
  - stylix
  - theming
  - protocol
source:
---

# The Ricing Protocol — how a song gets made and kept honest

A first-class house rule: ricing is not "pick some hex codes and hope."
It is a small protocol with a separation of concerns, and one mandatory
check whenever a rice or song changes.

> **Scope.** This page is protocol: how a rice is made and kept honest — the
> creation/application split and the mandatory vision-check. Per-song design
> memory (a palette's rationale, the iteration log) lives in the songbook, not
> here: cross-cutting memory in `song/songbook/` (see
> [[Self-Ricing#Songbook Discipline — the "Self" in Self-Ricing]]), per-song
> notes in `song/songbook/<name>/design/`. The worked examples below (the
> `sonata` derivation) illustrate the protocol; the songbook is the record —
> `song/songbook/sonata/design/intent.md` states the shipped key in full.

## 1. Two separated concerns: creating a base16, and applying it

**Creation** (deriving a base16 scheme from source material — a wallpaper, a
prompt, a mood) is a *different job* from **application** (fanning that
scheme out to every surface that reads it). Conflating them is how a rice
job ends up hard-coding colours in six different files that drift apart.

- **Creation** happens once, in the song's `rice.nix`, as `aoide.livery.base16`
  — sixteen literal hex slots (base00–base0F) plus the small `palette`
  convenience block (bg/fg/accent/urgent/hot). The **`sonata`** song
  (`song/songbook/sonata/rice.nix`) is the worked example — the LIGHT dusk
  key currently performed on yomi-strix: every slot is keyed by eye from the
  wallpaper (`song/covers/yuki-sonata.png` — a pianist at a grand piano on
  mirror-still water at dusk) with a comment naming which region of the image it
  reads from. The pale peach-cream **sunlit cloudbank** → `base00`; the
  **piano's near-black**, read as a plum ink → `base05`/`fg`; the **dusk
  slate-blue** upper sky → `base0D`/`accent`; the **horizon's green transition
  band** → `base0B`/`hot` (the one-blaze trace colour); the **crimson
  piano-stool cushion** → `base08`/`urgent`; and the **ember horizon**, the
  **cloud-gold** highlight, the **cool-sky cyan-teal**, the **plum-mauve** cloud
  and the **warm rust** of the deep shadow fill the rest of the ramp. The cover
  lives in the shared `song/covers/` library any song references by literal
  path. **Status:** there is no automated derivation of a base16 key from a
  wallpaper — no `rice gen` was ever built (cut outright, not left as a
  stub — see [[aoide-cli]]). Creation is a human/agent reading the source
  image and writing the sixteen slots by hand, once, in one file.
- **Application** is [[Stylix]]'s job, and only Stylix's: one `base16Scheme`
  feeds every nix-manageable target (terminal, GTK/Qt, icons, cursor,
  editors, browser, boot) automatically. On the Quickshell side, the same
  same livery values fan out through `stage/livery.json` — one runtime read,
  every QML surface. **No other file hard-codes a colour available from
  livery.** A dendrite or facet that wants a colour reads
  `aoide.livery.*`; it never writes its own hex.

The point of the split: creation is where taste and vision-checking live
(this section, below); application is mechanical and never needs re-deriving
per surface. When a rice looks wrong, ask which concern broke — usually it is
application (a surface reading a livery role it does not own, or hosting
a stray literal) rather than creation (the sixteen slots themselves).

## 2. The mandatory vision-check

Whenever a rice or song changes, vision-check it before calling it done — a
compile-clean rebuild is not the same as a rice that reads correctly. Two
things to look at, side by side, on the live desktop:

1. **Terminals and the shell UI agree on light/dark.** A rice keys
   `stylix.polarity` (`"light"` or `"dark"`) once; every surface must read as
   the *same* polarity, checked by eye every time — opacity, blur, and gloss
   gradients can each independently push a surface's apparent brightness away
   from its declared polarity. The light key holds a bright cream Aero-glass terminal
   (kitty `background_opacity` 0.86) next to a cream frosted-glass bar and its
   popouts at their own, more transparent opacity (0.45 / 0.45); the cream `base00` is
   `#f4e9e2` (sonata's peach-cream). **Brightness is opacity, not colour**: how much of the dim
   wallpaper is allowed to show through, not the hex value — a surface can
   read "too dark" and still be exactly the right colour, so the fix for a
   muddy surface is opacity/glass, not a whiter hex. hyprglass glasses the
   **windows** too — `manage_window_blur = 1` in the compositor facet extends
   the Liquid-Glass refraction/fresnel from the quickshell layer surfaces onto
   the translucent terminal, so terminal and shell wear one glass (the shader
   only paints visible translucent content, so opaque windows are untouched).
   A `light { glass_opacity }` preset override brightens that glass under the
   light polarity.

   **Everything is edged.** Hard square corners are the house style — the global
   Hyprland decoration `rounding` is 0, the kitty windowrule pins `rounding 0`
   too, and every quickshell surface (bar, dock panes, gadget frames, popouts,
   notification/OSD cards, workspace highlight) sets `radius: 0`. No surface
   rounds; a stray rounded corner reads as a surface that missed the grammar.
2. **Widget colours match the bar.** Every gadget, popout, and dock surface
   pulls from the same `aoide.livery.*` roles the bar uses (the house
   grammar's glyph/role palette, recorded historically in the retired
   `default` song's design memory — now `references/pantheon/pantheon-grammar.md`
   — see [[Song-Anatomy]] — `wireCyan`, `holoBlue`, `violet`, `glitchPink`,
   `paletteAccent`/`paletteHot`). A widget that
   *looks* subtly off (a slightly different cream, an accent that reads as a
   different hue) usually means it resolved a fallback instead of the song's
   actual livery value — the fix is in the livery wiring, not a local hex
   tweak.

This is a **vision check**, not a lint rule: it means actually looking at the
running desktop (screenshot or live) after a rice change, not just trusting
that `nix flake check` passed. `adcheck` catches structural violations (a
facet reading another module, a surface with two owners); it cannot catch
"the terminal reads dark while the bar reads light."

## Related

- [[Stylix]] — the application half: one base16 scheme, baked fan-out.
- [[livery (rename to lyra)]] — the livery seam creation writes into and application reads from.
- [[Song-Vocabulary]] — key/song/cover vocabulary this protocol operates on.
- [[Song-Anatomy]] — where the songbook and per-song design memory live under
  `song/`.
- [[Self-Ricing]] — the songbook write-back loop, and the drafts mechanism
  for iterating on a key before it's declared.
