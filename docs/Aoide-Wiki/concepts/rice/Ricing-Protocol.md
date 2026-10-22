---
type: concept
created: 2026-07-28
updated: 2026-08-25
tags:
  - aoide
  - rice
  - rice
  - stylix
  - theming
  - protocol
source:
---

# The Ricing Protocol — how a rice gets made and kept honest

A first-class house rule: ricing is not "pick some hex codes and hope."
It is a small protocol with a separation of concerns, and one mandatory
check whenever a rice or rice changes.

> **Scope.** This page is protocol: how a rice is made and kept honest — the
> creation/application split and the mandatory vision-check. Per-rice design
> memory (a palette's rationale, the iteration log) lives in the rices, not
> here: cross-cutting memory in `rices/` (see
> [[Self-Ricing#Rices Discipline — the "Self" in Self-Ricing]]), per-rice
> notes in `rices/<name>/design/`. The worked examples below (the
> `sonata` derivation) illustrate the protocol; the rices is the record —
> `rices/sonata/design/intent.md` states the shipped key in full.

## 1. Two separated concerns: creating a base16, and applying it

**Creation** (deriving a base16 scheme from source material — a wallpaper, a
prompt, a mood) is a *different job* from **application** (fanning that
scheme out to every surface that reads it). Conflating them is how a rice
job ends up hard-coding colours in six different files that drift apart.

- **Creation** happens once, in the rice's `rice.nix`, as `strillix.theme.base16`
  — sixteen literal hex slots (base00–base0F) plus the small `palette`
  convenience block (bg/fg/accent/urgent/hot). The **`sonata`** rice
  (`rices/sonata/rice.nix`) is the worked example — the LIGHT Greek key, a
  temple in daylight, authored by hand as a register rather than read off an
  image, with a comment on each slot naming its role. The pale warm **marble
  ground** → `base00`/`bg` (`#f2ebde`); the **plum-charcoal ink** → `base05`/`fg`;
  the **Attic gold** chrome accent → `base0A`/`accent` (`#a07414`), which is also
  the active window frame; the **aegean blue**, stepped back to a preview/info
  role → `base0D`; the **bronze-verdigris** structural outline → `base0C`, which
  is the inactive window frame (`window.borderInactive`); the **laurel
  leaf-green** one-blaze trace → `base0B`/`hot`; and the **terracotta** urgent
  → `base08`/`urgent`. sonata declares `strillix.theme.polarity = "light"`
  beside the palette and no wallpaper (`strillix.theme.wallpaper = null`), so
  the stylix lane bakes a solid from `palette.bg`. **Status:** there is no
  automated derivation of a base16 key from a wallpaper — no `rice gen` was
  ever built (cut outright, not left as a stub — see [[aoide-cli]]). Creation
  is a human/agent reading the source material and writing the sixteen slots
  by hand, once, in one file.
- **Application** is [[Stylix]]'s job, and only Stylix's: one `base16Scheme`
  feeds every nix-manageable target (terminal, GTK/Qt, icons, cursor,
  editors, browser, boot) automatically. On the Quickshell side, the same
  same theme values fan out through `stage/theme.json` — one runtime read,
  every QML surface. **No other file hard-codes a colour available from
  theme.** A dendrite that wants a colour reads
  `strillix.theme.*`; it never writes its own hex.

The point of the split: creation is where taste and vision-checking live
(this section, below); application is mechanical and never needs re-deriving
per surface. When a rice looks wrong, ask which concern broke — usually it is
application (a surface reading a theme role it does not own, or hosting
a stray literal) rather than creation (the sixteen slots themselves).

## 2. The mandatory vision-check

Whenever a rice or rice changes, vision-check it before calling it done — a
compile-clean rebuild is not the same as a rice that reads correctly.
`strx reload` is how a change reaches the live desktop to check at all: edit
→ `strx reload` → look, the one mode-aware command that snapshots the
current rice, syncs it live, and reloads Quickshell regardless of whether
the rice is staged or drafted ([[Self-Ricing#The Rice Loop]] has the full
loop). Two things to look at, side by side, on the live desktop:

1. **Terminals and the shell UI agree on light/dark.** A rice declares
   `strillix.theme.polarity` beside its palette, and the stylix lane hands it to
   `stylix.polarity`; every surface must read as
   the *same* polarity, checked by eye every time — opacity, blur, and gloss
   gradients can each independently push a surface's apparent brightness away
   from its declared polarity. The light key, sonata, has a pale marble ground
   (`base00` `#f2ebde`), and not every surface is translucent: its bar is an
   opaque flat strip (alpha 1.0), its popouts and gadget frames are glass at 0.72,
   and its kitty terminal sits at `background_opacity` 0.86. **Brightness is
   opacity, not colour**: how much of whatever sits behind a translucent surface
   shows through (the windows and wallpaper behind it), not the hex value — a
   surface can read "too dark" and still be exactly the right colour, so the fix for a
   muddy surface is opacity/glass, not a whiter hex. hyprglass glasses the
   **windows** too — `manage_window_blur = 1` in the compositor lane extends
   the Liquid-Glass refraction/fresnel from the quickshell layer surfaces onto
   the translucent terminal, so terminal and shell wear one glass (the shader
   only paints visible translucent content, so opaque windows are untouched).
   The compositor lane's hyprglass block also sets a fixed `light` preset
   (`glass_opacity = 0.82`) that no rice gates: a slightly brighter frost for
   the light keys.

   **Everything is edged.** Hard square corners are the house style — the global
   Hyprland decoration `rounding` is 0 for a rice that sets none, the kitty
   windowrule follows that same value, and every house-style quickshell surface
   (bar, dock panes, gadget frames, popouts, notification/OSD cards, workspace
   highlight) sets `radius: 0`. No house-style surface rounds; a stray rounded
   corner reads as a surface that missed the grammar. barcarolle is the user's
   exception: it sets `strillix.theme.geometry.rounding = 10`, which rounds its
   window corners (the kitty windowrule with them), and its widget bodies carry
   their own radii.
2. **Widget colours match the bar.** Every gadget, popout, and dock surface
   pulls from the same `strillix.theme.*` roles the bar uses (the house
   grammar's glyph/role palette, recorded historically in the retired
   `default` rice's design memory — now `references/pantheon/pantheon-grammar.md`
   — see [[Rice-Anatomy]] — `wireCyan`, `holoBlue`, `violet`, `glitchPink`,
   `paletteAccent`/`paletteHot`). A widget that
   *looks* subtly off (a slightly different cream, an accent that reads as a
   different hue) usually means it resolved a fallback instead of the rice's
   actual theme value — the fix is in the theme wiring, not a local hex
   tweak.

This is a **vision check**, not a lint rule: it means actually looking at the
running desktop (screenshot or live) after a rice change, not just trusting
that `nix flake check` passed. `adcheck` catches structural violations (a
lane reading another module, a surface with two owners); it cannot catch
"the terminal reads dark while the bar reads light."

## Related

- [[Stylix]] — the application half: one base16 scheme, baked fan-out.
- [[theme]] — the theme seam creation writes into and application reads from.
- [[Rice-Vocabulary]] — key/rice/wallpaper vocabulary this protocol operates on.
- [[Rice-Anatomy]] — where the rices and per-rice design memory live under
  `rice/`.
- [[Self-Ricing]] — the rices write-back loop, and the drafts mechanism
  for iterating on a key before it's declared.
