---
type: design
created: 2026-07-28
tags: [aoide, rice, song, stylix, theming, protocol]
source: "[[references/AOIDE-HANDOFF]]"
---

# The Ricing Protocol — how a song gets made and kept honest

A first-class house rule, promoted out of the light-theme rework
([[Pantheon-Grammar]] round 5): ricing is not "pick some hex codes and hope."
It is a small protocol with a separation of concerns, and one mandatory
check whenever a rice or song changes.

## 1. Two separated concerns: creating a base16, and applying it

**Creation** (deriving a base16 scheme from source material — a wallpaper, a
prompt, a mood) is a *different job* from **application** (fanning that
scheme out to every surface that reads it). Conflating them is how a rice
job ends up hard-coding colours in six different files that drift apart.

- **Creation** happens once, in the song's `rice.nix`, as `aoide.notes.base16`
  — sixteen literal hex slots (base00–base0F) plus the small `palette`
  convenience block (bg/fg/accent/urgent/hot). The hero song
  (`song/repertoire/hero/rice.nix`) is the worked example: every slot is
  keyed by eye from the wallpaper (Alma-Tadema's *Unconscious Rivals*) with a
  comment naming which region of the painting it reads from (cream vault →
  base00, umber shadow → base05, cornflower sky-glaze → base0D, sage leaf →
  base0B/hot, muted rose flesh-tone → base08/urgent). `aoide rice gen` is the
  eventual automated form of this same step (still a stub — see
  [[aoide-cli]]); until it lands, creation is a human/agent reading the
  source image and writing the sixteen slots by hand, once, in one file.
- **Application** is [[Stylix]]'s job, and only Stylix's: one `base16Scheme`
  feeds every nix-manageable target (terminal, GTK/Qt, icons, cursor,
  editors, browser, boot) automatically. On the Quickshell side, the same
  notes fan out through `stage/notes.json` — one runtime read, every QML
  surface. **No other file should ever hard-code a colour that could instead
  be read from notes.** A dendrite or facet that wants a colour reads
  `aoide.notes.*`; it never writes its own hex.

The point of the split: creation is where taste and vision-checking live
(this section, below); application is mechanical and should never need
re-deriving per surface. When a rice looks wrong, ask which concern broke —
usually it is application (a surface reading a note it shouldn't, or hosting
a stray literal) rather than creation (the sixteen slots themselves).

## 2. The mandatory vision-check

**Whenever a rice or song changes, vision-check it before calling it done —
a compile-clean rebuild is not the same as a rice that reads correctly.**
Two things to look at, side by side, on the live desktop:

1. **Terminals and the shell UI agree on light/dark.** A rice keys
   `stylix.polarity` (`"light"` or `"dark"`) once; every surface must read as
   the *same* polarity. The light key shipped this way: a bright cream
   Aero-glass terminal (kitty `background_opacity`) next to a cream
   frosted-glass bar and its popouts at a *close* opacity — checked by eye
   every time, because opacity + blur + gloss gradients can each independently
   push a surface's apparent brightness away from its declared polarity.
   "Close" beats "identical-on-paper but wrong on screen." The opacity has been
   re-tuned against the live desktop more than once: first *down* together from
   0.72 to kitty 0.60 / bar 0.58 (too transparent — the warm painting bled
   through and the surfaces read beige, not light). The instinct after that was
   to lighten the *colour* (whiten `base00`), but khoa's correction drew the
   real distinction: **keep the cream-and-ink look, just make the surface read
   brighter.** Brightness is not colour — it is how much of the dim wallpaper is
   allowed to show through. So the cream `base00` stayed `#f4ecdc`, and the
   terminal was brightened by *opacity* instead (kitty `background_opacity`
   0.86): the same cream, now dominant over the painting rather than muddied
   dim by it. The lesson: reach for opacity/glass before you reach for a whiter
   hex — a surface can read "too dark" and still be exactly the right colour.
   hyprglass now glasses the **windows** too — `manage_window_blur = 1` in the
   compositor facet extends the Liquid-Glass refraction/fresnel from the
   quickshell layer surfaces onto the translucent terminal, so terminal and
   shell wear one glass (the shader only paints visible translucent content, so
   opaque windows are untouched). A `light { glass_opacity }` preset override
   brightens that glass under the light polarity.

   **Everything is edged.** Hard square corners are the house style — the global
   Hyprland decoration `rounding` is 0, the kitty windowrule pins `rounding 0`
   too, and every quickshell surface (bar, dock panes, gadget frames, popouts,
   notification/OSD cards, workspace highlight) sets `radius: 0`. No surface
   rounds; a stray rounded corner reads as a surface that missed the grammar.
2. **Widget colours match the bar.** Every gadget, popout, and dock surface
   pulls from the same `aoide.notes.*` roles the bar uses
   ([[Pantheon-Grammar]]'s glyph/role grammar: `wireCyan`, `holoBlue`,
   `violet`, `glitchPink`, `paletteAccent`/`paletteHot`). A widget that
   *looks* subtly off (a slightly different cream, an accent that reads as a
   different hue) usually means it resolved a fallback instead of the song's
   actual note — the fix is in the note wiring, not a local hex tweak.

This is a **vision check**, not a lint rule: it means actually looking at the
running desktop (screenshot or live) after a rice change, not just trusting
that `nix flake check` passed. `adcheck` catches structural violations (a
facet reading another module, a surface with two owners); it cannot catch
"the terminal reads dark while the bar reads light."

## Related

- [[Pantheon-Grammar]] — the visual grammar (glyphs, depth recipe, roles) this
  protocol keeps coherent across surfaces.
- [[Stylix]] — the application half: one base16 scheme, baked fan-out.
- [[Notes]] — the note seam creation writes into and application reads from.
- [[Song-Vocabulary]] — key/song/cover vocabulary this protocol operates on.
- [[Self-Ricing]] — the automated future of the creation step (`rice gen`).
