---
type: design
created: 2026-07-27
updated: 2026-07-28
tags: [aoide, design, rice, song, qml, glyph, pantheon]
source: "[[references/AOIDE-HANDOFF]]"
---

# Pantheon Grammar

The visual language for Aoide's first-song widgets: **hollow wireframe depth**
laid over the Windows-7 Aero-glass base, plus a **glyph grammar** where every
music glyph carries meaning. Named for the reference stills at
`references/pantheon/*.png` — outlined volumes drawn with lines, 3D read from
stacked offset outline copies, translucent panes with lowercase callout labels
on angled leaders, and ONE neon accent reserved for the live/hot element.

> **This is song-agent design memory, mirrored here.** A visual grammar is a
> *cross-cutting design decision* about how songs look — exactly the material
> the **songbook** is for. Under the model the dev wiki documents architecture
> and protocol; **per-song and cross-cutting design memory belongs to the song
> agent in a songbook under `song/`** (`song/songbook/` for cross-cutting
> grammar like this one, `song/songbook/<name>/design/` for one song's notes —
> see [[Song-Anatomy]], [[Self-Ricing#Songbook Discipline — the "Self" in Self-Ricing]]).
> This page is retained as a reference mirror while that songbook is still
> **sparse and aspirational** (`song/songbook/` currently holds only a
> placeholder); the content's true home is `song/`, and its migration there is
> pending (flagged for the handoff). The wireframe/QML *constants* below are
> load-bearing architecture and stay documented in the wiki regardless.

The glass stays. The wireframe depth goes *over and around* it — it does not
replace the blur/gloss/frost.

---

## 1. The depth recipe

Behind each glass panel sit **two hollow outline copies**, offset down-right at
decreasing opacity — the "stacked offset volume" read. Border only, no fill, no
MouseArea (they never intercept input). Declared *before* the glass so they
render behind it: only the sliver past the panel's bottom-right edge shows as a
clean outline; the rest reads as a faint double-rule ghost through the glass.

Constants (the tunable seam — all in `GadgetFrame.qml`):

| constant        | value | meaning                                  |
|-----------------|-------|------------------------------------------|
| `depthOff1`     | 3 px  | near copy offset (down-right)            |
| `depthOff2`     | 6 px  | far copy offset (down-right)             |
| `depthOpacity1` | 0.35  | near copy border opacity                 |
| `depthOpacity2` | 0.18  | far copy border opacity                  |
| `depthExtent`   | =off2 | headroom a clipping container must add   |

Border color is always `drachma.paletteAccent`; radius matches the panel (4 for
gadget frames, 6 for the dock container body).

**Where it lives:**

- `GadgetFrame.qml` — the core. Every dock gadget, bar popout, and floating
  gadget inherits it (title/footer box math untouched).
- `AoideAgentWidgets.qml` — the dock container body carries its own copy of the
  recipe (radius 6); the 10 px content margin absorbs the offset so nothing
  clips.
- `BarPopout.qml` / `DesktopGadgets.qml` — add `depthExtent` of right/bottom
  headroom (window / delegate grown, frame pinned top-left at its exact width)
  so the back copy is not cut off by the surface bounds.
- `AoideBar.qml` — the subtle bar-scale cousin: a second, dimmer hairline 2 px
  above the bottom edge rule. Two parallel lines = a stacked slab.

To tune the whole rig, edit the five constants in `GadgetFrame.qml`.

---

## 2. The glyph grammar

Glyphs carry **meaning**, never decoration. Three tiers plus one seam:

### State tier — `baton/theme.rs` vocabulary (verbatim)

| glyph | state                                   |
|-------|-----------------------------------------|
| `♪`   | working (running / active / tool)       |
| `𝄐`   | awaiting (await / block / notification) |
| `𝄽`   | idle                                    |
| `𝄂`   | done (also `stop`)                      |
| `·`   | unknown                                 |

Used by `BatonGadget`, `DagGraphGadget` (session nodes wear the state glyph),
and `TerminalManagerGadget` (round 2 — the state glyph replaced the old
`[running]`/`[awaiting]`/`[done]` word-badges; cwd + elapsed drop to a dim
lowercase callout line). Kept in lockstep with the Rust so every surface reads
the same score.

### Workspace tones

The musical-notation workspace cells (`WorkspaceRow`) — one tone per workspace.
Identity; untouched.

### Kaomoji identity

The bar's empty-title kaomoji rotation (re-rolled at random on every
active-window change — see round 5, below) is the dxflake identity — kept.
`BatonGadget`'s "nothing to conduct ♪(´ε｀ )" is its own empty-state note — kept.

### Seam tier — one footer divider per container

At most **one** staff-run seam divider per container, in the footer:

- Dock (`AoideAgentWidgets`): `𝄂𝄚𝅦𝄚𝄞𝅄` footer run — the container's one seam.
- `BatonGadget`: the `𝄂𝄚𝅦𝄚` status-line prefix — the pane's one seam.
- `GadgetFrame` draws a *structural* `╚═══╝` box footer (box-drawing, not an
  ornament glyph) — that is chrome, not a seam glyph.

### The bar's clef

`𝄞` stays — it IS the power-menu button (it has a job). So do the functional
note-glyphs: volume (`♩~ ♪~ ♫~ ♬~`), battery rests, network (`𝆹𝅥𝅮`/`𝆺𝅥𝅯`).

---

## 3. What was removed, and why

Gratuitous ornaments — glyphs with no job — were deleted:

| removed                | file                  | why                                    |
|------------------------|-----------------------|----------------------------------------|
| `ৎ𝄢` interior end-cap  | `GadgetFrame.qml`     | decorative; box footer already closes it |
| `ৎ𝄢` left end-cap      | `AoideBar.qml`        | pure decoration at the strip's left    |
| `𝄚𝅦𝄚𝄂` right end-cap   | `AoideBar.qml`        | pure decoration at the strip's right   |
| `◆` / `●` node markers | `DagGraphGadget.qml`  | replaced by *meaningful* forms: double-rule outline = project, single = session; session state glyph = live state |
| `├─ │ └─` limb glyphs  | `DagGraphGadget.qml`  | round 2 — replaced by a *drawn* leader (a per-row Canvas painting the refs' kinked elbow into the indent gutter), so edges read as neon lines, not text |
| `[running]`/`[awaiting]`/`[done]` word-badges | `TerminalManagerGadget.qml` | round 2 — text badges violated the state tier; replaced by the baton state glyphs (`♪ 𝄐 𝄽 𝄂`) |

---

## 4. The DAG showpiece (`DagGraphGadget`)

The Pantheon effect at full strength:

- **Nodes** are small hollow outline boxes. **Double-ruled** (outer + inset
  border) = project; **single** = session.
- **Edges** are **drawn leaders** (round 2): a per-row `Canvas` paints the refs'
  signature kinked elbow into the indent gutter — full-height continuation rules
  for ancestors with a later sibling, then this row's branch: a vertical drop
  that taps a 45° kink into the child box, ending in a ~2 px terminal dot. Accent
  color; opacity matched to the child's dim level (hot child = brighter leader).
  Cheap: one Canvas per row, `requestPaint` only on trace/size change, never
  per-frame. (Replaces the old `├─ │ └─` box-drawing text limbs.) Tunable seam:
  `indentStep` 16 · `leaderKink` 4 · `leaderDot` 2 · `leaderWidth` 1.5.
- **Labels** are the node's live name/agent inside the box, plus a dim
  lowercase **callout id** built from the real node id (`session:9f3a…` →
  `session.9f3a…`, `project:aoide` → `project.aoide`) — the reference's
  `optic nerve.LE.dk.002` token pattern, from real ids.
- **Neon dominance** (round 2 — the refs' core rule, one blaze against a dim
  field): the non-hot layers rest LOW — project double-rule `dimProject` 0.45,
  idle session `dimIdle` 0.35, done `dimDone` 0.2, callout label `dimCallout`
  0.35. When nothing is hovered, the whole graph rests dim.
- **The one neon**: exactly the traced/live node (driven by
  `shared.tracedSessionId`, set on `TerminalManager` row hover) **blazes** —
  `border.width` 2, full-bright accent, bold label, faint accent fill, AND a
  soft **halo**: two transparent rings, the box grown +2/+4 px, accent border at
  `haloOpacity1` 0.35 / `haloOpacity2` 0.15 (the depth-stack trick used as a glow
  instead of an offset). The same push lands on the **TERMINALS** traced row
  (border 2 + matching halo), so the trace link reads as THE hot element across
  both gadgets.

## Round 4 — the multicolor field + the bar joins (2026-07-27)

- **base16 through the note seam**: the stage notes carry an optional
  all-or-nothing `base16` block (drachma-validated). DrachmaState maps four
  semantic roles from it — `wireCyan` (base0C, structural outlines/leaders),
  `holoBlue` (base0D, depth-stack back copies + link callouts), `violet`
  (base0E, DAG project volumes), `glitchPink` (base08, urgent/glitch) — each
  falling back to paletteAccent when the block is absent. Restraint rule
  holds: the colors are ROLES, not decoration; hot (base0B green) stays the
  one blaze.
- **Vanishing-point depth direction**: GadgetFrame gained `depthDx`/`depthDy`;
  every surface leans its offset stack TOWARD screen centre (960,540). The
  dock leans right, floating gadgets compute their lean from their own centre
  live during drag (flips across the midline), the bar's slab hairlines
  project down.
- **Orchestration vocabulary**: callout titles say what the thing conducts —
  `gadgets.case`, `baton.control`, `terminals.roster`, `dag.trace`,
  `volume.level`, `battery.gauge`, `clock.face`, `nowplaying.score`,
  `meters.pulse`, `power.reserve`, `calendar.sheet`. Body words are out;
  orchestration words are in.
- **The bar in the grammar** (khoa: centered workspaces, clef that fits, no
  rose remnants): workspace numbers sit at TRUE screen centre (the bar-scale
  echo of the vanishing point), active cell boxed `wireCyan`, urgent pulses
  `glitchPink`; the ✎N live-sessions cell is the bar's ONE hot element
  (paletteHot green — it IS the live thing); tray/calendar open-states accent
  `wireCyan`; hairline + depth echo `wireCyan`. The 𝄞 clef power glyph is
  16px — the largest size whose paint extent (~1.8× em) clears the 36px strip
  (a 52px pop-out apron was tried and rolled back: the transparent apron let
  the wallpaper's centre-lines bleed through under the hairline). Music
  seasoning kept where it has a job: clef = power, ♬⋆.˚/ᝰ.ᐟ special marks,
  kaomoji title, ♪𝄐𝄽𝄂 state glyphs.
- **Real data only**: the session write door (`aoide graph session
  start/phase/end/hook`) exists now — widgets read the LIVE stage; demo
  fixture files are retired permanently. Forced-state screenshots register
  short-lived real sessions and `graph prune` after.

## Round 5 — the light key: from dusk to cream (2026-07-28)

The desktop flipped its base key from dark to a LIGHT warm classical-academic
register, keyed off the new main wallpaper: Alma-Tadema's *Unconscious
Rivals*. `stylix.polarity = "light"` (base facet default); the **`sonata`** song
(`song/songbook/sonata/rice.nix`, the light key selected on yomi-strix)
supplies the sixteen base16 slots read from the painting — cream/parchment
`base00`, deep umber ink `base05`/`fg`, dusty cornflower `base0D`/`accent`, sage
green `base0B`/`hot` (the one-blaze trace colour), muted rose `base08`/`urgent`,
plus terracotta/ochre/teal/plum filling the rest of the ramp. `hero` is its own
separate dusk-plum key; a dark `moonlight-sonata` counterpart to `sonata` is
planned (khoa, not yet built). This is the worked example for
[[design/Ricing-Protocol|the Ricing Protocol]]'s creation step — see that page
for the base16-derivation discipline and the mandatory light/dark vision-check
this rework introduced as a house rule.

**The bar reborn as one bar of music.** `WorkspaceRow.qml` replaced the old
hollow ovals with **solid, distinct musical note glyphs per workspace id**
(♩ ♪ ♫ ♬ 𝅘𝅥𝅮 𝅘𝅥𝅯, repeating past the set; `magic`/`scratch` get their own marks
and ride a ledger line above the staff). Each note sits at a staff degree that
rises with its id, so open workspaces read as an ascending run on a real
five-line staff. Resting notes are solid umber ink; the **active** workspace's
note **swells** (15px → 19px) and **fills with the song's accent colour**,
resting on a soft accent glow (the old black playhead, re-cast); urgent
workspaces pulse `glitchPink`. The whole strip is now a **cream frosted-glass
sheet**: `paletteBg` alpha-blended (tuned down from an initial 0.72 to a more
transparent **0.58**, close to kitty's own glass below — checked by eye each
time, per [[design/Ricing-Protocol|the Ricing Protocol]], not assumed equal
just because both read "0.7-ish"), hard square corners (no round), a defined
black rail, and the sanctioned Aero gloss gradient on top. The bar's popouts
(now-playing, volume, battery, calendar) followed suit — dropped their opaque
white-sheet chrome for the same cream frosted glass (0.60) with umber ink, so
every surface (bar, popouts, terminal) reads as one glass rather than a bar
and a set of un-matched dialogs.

**Kitty rides the compositor blur (Aero-glass terminal).** `background_opacity`
on the kitty dendrite (first tuned down from an initial 0.72 to 0.60 in step
with the bar, then **re-tuned up to 0.86** — the later correction recorded in
[[design/Ricing-Protocol#2. The mandatory vision-check]]: the terminal read too
transparent/beige, and *brightness is opacity, not colour*, so the cream stayed
and the surface was made dominant over the painting by opacity) makes the cell
background translucent while glyphs stay fully opaque/crisp; Hyprland's own blur
(global `decoration:blur`) frosts behind it.
Hyprland 0.56 changed its windowrule matcher syntax — the old
`class:^(kitty)$` form is rejected outright ("invalid field ... missing a
value"); the compositor facet now uses `match:class kitty` for both the
opacity rule (focused 1.0 / unfocused 0.90 — a gentle Aero defocus fade) and
the rounding rule. kitty's own `background_blur` stays off (it is a
macOS/KDE-only path, inert under Hyprland) — the compositor does the frosting.

**The empty-title kaomoji hums at random, not hourly.** The bar's
empty-window-title kaomoji identity (kept from dxflake, [[Pantheon-Grammar]]
§2) now re-rolls on every active-window change instead of a fixed hourly
rotation — a small liveliness fix, same kaomoji set.

**The wallpaper survives a rebuild.** The wallpaper layer used to lose its
image after every rebuild because nothing re-seeded the live
`stage/cover.json` from the song's baked wallpaper. Fixed by exporting
`AOIDE_WALLPAPER=${config.aoide.drachma.wallpaper}` on the Quickshell facet's
systemd unit (null wallpaper → no env, degrading cleanly) — `AoideWallpaper`
reads it on boot as the seed, with the live stage file still free to override
it at rehearsal.

**The fetch got small.** The fastfetch greeting (`modules/dendrites/
fastfetch/`) is now a directory dendrite carrying its own bundled logo
asset — a compact 13×7 redraw of Aoide's lyre (three strings, curved arms, a
soundbox, the A·O·I·D·E ground) sized so the info column sits flush beside it
without wrapping in a tiled/narrow terminal, plus aligned key columns and two
subtly music-marked section rules (`♪ hardware`, `♪ software`).

## Related

- [[design/Ricing-Protocol|Ricing Protocol]] — the creation/application split
  and the vision-check this grammar is kept coherent by.
- [[Song-Anatomy]] — the songbook under `song/` where this design memory
  belongs; the destination of the pending migration.
- [[Self-Ricing]] — the songbook write-back loop (the "self" in self-ricing).
- [[Notes]] — the `aoide.drachma` seam every surface here reads its roles from.
- [[Gadget-Dock]] · [[Terminal-Commander]] — the surfaces that wear the grammar.
