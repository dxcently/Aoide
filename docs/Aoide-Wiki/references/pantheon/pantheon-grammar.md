# Pantheon Grammar — the retired `default` song's design language

**Status (2026-08-14): historical reference, not a live grammar.** This
page was `song/songbook/default/design/pantheon.md` — the design memory of
the `default` song, which owned the house grammar back when `default` was
the shipped standard. `default` has since been retired outright (renamed to
`sonata`, which draws its own, deliberately divergent grammar — see
`song/songbook/sonata/design/greek-grammar.md`). Nothing in the live tree
instantiates this grammar anymore; it is kept here, relocated out of
`song/songbook/` (which no longer has a `default/` to hold it), as source
material for whoever picks up the wireframe-depth idiom again. The full
`default` song history — `rice.nix`, `livery.json`, iteration log — is
git-recoverable, not deleted.

**Original scope** (as authored): cross-cutting visual language every song
instantiated in its own key. A song's *current* instantiation was recorded
in that song's `design/intent.md` (sonata: `song/songbook/sonata/design/
intent.md`).
**Source stills:** `docs/Aoide-Wiki/references/pantheon/*.png` — outlined
volumes drawn with lines, 3D read from stacked offset outline copies,
translucent panes with lowercase callout labels on angled leaders, and ONE
neon accent reserved for the live/hot element.

The visual language for Aoide's widgets: **hollow wireframe depth** laid over
the Windows-7 Aero-glass base, plus a **glyph grammar** where every music
glyph carries meaning. The glass stays. The wireframe depth goes *over and
around* it — it does not replace the blur/gloss/frost.

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

Border color is always `livery.paletteAccent`; radius matches the panel (4 for
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

### State tier — `conductor/theme.rs` vocabulary (verbatim)

| glyph | state                                   |
|-------|-----------------------------------------|
| `♪`   | working (running / active / tool)       |
| `𝄐`   | awaiting (await / block / notification) |
| `𝄽`   | idle                                    |
| `𝄂`   | done (also `stop`)                      |
| `·`   | unknown                                 |

Used by `ConductorGadget`, `DagGraphGadget` (session nodes wear the state glyph),
and `TerminalManagerGadget` (cwd + elapsed drop to a dim lowercase callout
line). Kept in lockstep with the Rust so every surface reads the same score.

### Workspace tones

The musical-notation workspace cells (`WorkspaceRow`) — one tone per workspace.
Identity; untouched.

### Kaomoji identity

The bar's empty-title kaomoji rotation (re-rolled at random on every
active-window change) is the dxflake identity — kept. `ConductorGadget`'s
"nothing to conduct ♪(´ε｀ )" is its own empty-state note — kept.

### Seam tier — one footer divider per container

At most **one** staff-run seam divider per container, in the footer:

- Dock (`AoideAgentWidgets`): `𝄂𝄚𝅦𝄚𝄞𝅄` footer run — the container's one seam.
- `ConductorGadget`: the `𝄂𝄚𝅦𝄚` status-line prefix — the pane's one seam.
- `GadgetFrame` draws a *structural* `╚═══╝` box footer (box-drawing, not an
  ornament glyph) — that is chrome, not a seam glyph.

### The bar's clef

`𝄞` stays — it IS the power-menu button (it has a job). So do the functional
note-glyphs: volume (`♩~ ♪~ ♫~ ♬~`), battery rests, network (`𝆹𝅥𝅮`/`𝆺𝅥𝅯`).

---

## 3. What the grammar excludes, and why

Gratuitous ornaments — glyphs with no job — are absent from the grammar:

| excluded               | file                  | why                                    |
|------------------------|-----------------------|----------------------------------------|
| `ৎ𝄢` interior end-cap  | `GadgetFrame.qml`     | decorative; box footer already closes it |
| `ৎ𝄢` left end-cap      | `AoideBar.qml`        | pure decoration at the strip's left    |
| `𝄚𝅦𝄚𝄂` right end-cap   | `AoideBar.qml`        | pure decoration at the strip's right   |
| `◆` / `●` node markers | `DagGraphGadget.qml`  | meaningless; the grammar uses *meaningful* forms instead: double-rule outline = project, single = session; session state glyph = live state |
| `├─ │ └─` limb glyphs  | `DagGraphGadget.qml`  | the grammar uses a *drawn* leader instead (a per-row Canvas painting the refs' kinked elbow into the indent gutter), so edges read as neon lines, not text |
| `[running]`/`[awaiting]`/`[done]` word-badges | `TerminalManagerGadget.qml` | text badges violate the state tier; the grammar uses the conductor state glyphs (`♪ 𝄐 𝄽 𝄂`) instead |

---

## 4. The DAG showpiece (`DagGraphGadget`)

The Pantheon effect at full strength:

- **Nodes** are small hollow outline boxes. **Double-ruled** (outer + inset
  border) = project; **single** = session.
- **Edges** are **drawn leaders**: a per-row `Canvas` paints the refs'
  signature kinked elbow into the indent gutter — full-height continuation rules
  for ancestors with a later sibling, then this row's branch: a vertical drop
  that taps a 45° kink into the child box, ending in a ~2 px terminal dot. Accent
  color; opacity matched to the child's dim level (hot child = brighter leader).
  Cheap: one Canvas per row, `requestPaint` only on trace/size change, never
  per-frame. Tunable seam:
  `indentStep` 16 · `leaderKink` 4 · `leaderDot` 2 · `leaderWidth` 1.5.
- **Labels** are the node's live name/agent inside the box, plus a dim
  lowercase **callout id** built from the real node id (`session:9f3a…` →
  `session.9f3a…`, `project:aoide` → `project.aoide`) — the reference's
  `optic nerve.LE.dk.002` token pattern, from real ids.
- **Neon dominance** (the refs' core rule, one blaze against a dim
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

---

## 5. The multicolor field, and the bar in the same grammar

- **base16 through the livery seam**: the stage livery carries an optional
  all-or-nothing `base16` block (livery-validated). LiveryState maps four
  semantic roles from it — `wireCyan` (base0C, structural outlines/leaders),
  `holoBlue` (base0D, depth-stack back copies + link callouts), `violet`
  (base0E, DAG project volumes), `glitchPink` (base08, urgent/glitch) — each
  falling back to paletteAccent when the block is absent. Restraint rule
  holds: the colors are ROLES, not decoration; hot (base0B green) stays the
  one blaze.
- **Vanishing-point depth direction**: `GadgetFrame` exposes `depthDx`/`depthDy`;
  every surface leans its offset stack TOWARD screen centre (960,540). The
  dock leans right, floating gadgets compute their lean from their own centre
  live during drag (flips across the midline), the bar's slab hairlines
  project down.
- **Orchestration vocabulary**: callout titles say what the thing conducts —
  `gadgets.case`, `conductor.control`, `terminals.roster`, `dag.trace`,
  `volume.level`, `battery.gauge`, `clock.face`, `nowplaying.score`,
  `meters.pulse`, `power.reserve`, `calendar.sheet`. Body words are out;
  orchestration words are in.
- **The bar in the grammar**: workspace numbers sit at TRUE screen centre
  (the bar-scale echo of the vanishing point), active cell boxed `wireCyan`,
  urgent pulses `glitchPink`; the ✎N live-sessions cell is the bar's ONE hot
  element (paletteHot — it IS the live thing); tray/calendar open-states
  accent `wireCyan`; hairline + depth echo `wireCyan`; the bar carries no
  second accent hue. The 𝄞 clef power glyph is 16px — the largest size whose
  paint extent (~1.8× em) clears the 36px strip without a pop-out apron: a
  transparent apron lets the wallpaper's centre-lines bleed through under the
  hairline. Music seasoning kept where it has a job: clef = power, ♬⋆.˚/ᝰ.ᐟ
  special marks, kaomoji title, ♪𝄐𝄽𝄂 state glyphs.
- **One bar of music**: `WorkspaceRow.qml` renders **solid, distinct musical
  note glyphs per workspace id** (♩ ♪ ♫ ♬ 𝅘𝅥𝅮 𝅘𝅥𝅯, repeating past the set;
  `magic`/`scratch` get their own marks and ride a ledger line above the
  staff). Each note sits at a staff degree that rises with its id, so open
  workspaces read as an ascending run on a real five-line staff. Resting
  notes are solid ink (`paletteFg`); the **active** workspace's note
  **swells** (15px → 19px) and **fills with the song's accent colour**,
  resting on a soft accent glow; urgent workspaces pulse `glitchPink`.
- **Real data only**: the session write door (`aoide graph session
  start/phase/end/hook`) exists; widgets read the LIVE stage; demo fixture
  files are retired permanently. Forced-state screenshots register
  short-lived real sessions and `graph prune` after.

---

## 6. The glass — one surface, everywhere

**Brightness is opacity, not colour.** A surface reads brighter by letting
less of the wallpaper through, never by whitening its tint. Every pane is the
song's `paletteBg` alpha-blended over the compositor's blur (hyprglass frosts
behind every layer):

- The bar is a single frosted-glass **sheet** — hard square corners (the
  house style: global `rounding` 0, every quickshell surface `radius: 0`; a
  stray rounded corner reads as a surface that missed the grammar), a defined
  black rail, and the sanctioned Aero gloss gradient on top (bright top,
  hard midline stop).
- The bar's popouts carry the **same glass at the same alpha** as the bar
  sheet, so bar, popouts, and terminal read as one glass rather than a bar
  and a set of un-matched dialogs.
- Kitty rides the compositor blur (Aero-glass terminal): translucent cell
  background, fully-opaque crisp glyphs; Hyprland's global `decoration:blur`
  frosts behind it. kitty's own `background_blur` stays off (a macOS/KDE-only
  path, inert under Hyprland) — the compositor does the frosting. Hyprland
  0.56 rejects the `class:^(kitty)$` windowrule matcher form outright
  ("invalid field ... missing a value"); the compositor facet uses
  `match:class kitty` for the opacity rule (focused 1.0 / unfocused 0.90 —
  a gentle Aero defocus fade) and the rounding rule.
- The exact alpha values are facet constants tuned against the performed
  key and recorded in that song's `design/intent.md` (the vision-check in
  the [Ricing Protocol](../../concepts/song/Ricing-Protocol.md)
  is what keeps them honest).

---

## 7. The fetch greeting

The fastfetch greeting (`modules/dendrites/fastfetch/`) is a directory
dendrite carrying its own bundled logo asset — a compact 13×7 redraw of
Aoide's lyre (three strings, curved arms, a soundbox, the A·O·I·D·E ground)
sized so the info column sits flush beside it without wrapping in a
tiled/narrow terminal, plus aligned key columns and two subtly music-marked
section rules (`♪ hardware`, `♪ software`).
