# Greek Grammar — the sonata house design language

**Song:** sonata (the Greek key; the selected song on yomi-strix).
**Scope:** sonata's OWN cross-cutting visual language — a deliberate divergence
from the default song's Pantheon wireframe grammar
(`song/songbook/default/design/pantheon.md`). Where Pantheon draws **hollow 3D
wireframe volumes** (offset outline stacks, neon leaders, a vanishing point),
sonata draws a **flat, typographic temple**: chrome built from **text and box
characters only** — columns, meanders, pediments — never a rendered 3D
wireframe. The colour register is Greek marble (see `design/intent.md`).

**What this is compatible with, and what it changes.** This grammar reads the
SAME `notes` role vocabulary (`paletteBg/Fg/Accent/Urgent/Hot`, `wireCyan`,
`holoBlue`, `violet`, `glitchPink`, `windowBorder`, `barBg`) and keeps **every
function, data source, and geometry identical** — same FileView seams, same
click targets, same 36px strip, same real-data discipline. It changes only the
**drawn chrome idiom** (wireframe → typographic-Greek) and the **callout dress**
(lowercase-dotted token gains a Greek order-mark). The **musical state-glyph
contract is untouched** — `♪ 𝄐 𝄽 𝄂 ·` stay verbatim (see §2).

---

## 1. The motif vocabulary

Five architectural motifs, all drawn from monospace text. They are the alphabet
every surface is reskinned from.

### Columns (the vertical order)

Panel side-rails and dividers are **columns**, not wireframe borders:

| glyph        | role                                             |
|--------------|--------------------------------------------------|
| `║`          | a full column shaft (a frame's left/right rail)  |
| `▌` `▐`      | an engaged half-column / pilaster at a panel edge |
| `│` `╎`      | a fluting line (repeated → a fluted shaft)       |
| `‖`          | a paired column / colonnade tick                 |

A fluted column is a run of `│`/`╎` a couple of cells wide; a colonnade is `║`
repeated with gaps. Columns carry `wireCyan` (bronze-verdigris) at rest — the
structural-outline role, dim.

### The meander (the Greek key / fret)

Horizontal **meander bands** run along friezes and seams, box-drawing only:

- fret pair — `⌐¬⌐¬⌐¬` (the smallest run, for tight seams)
- interlock band — `┏┛┗┓┏┛┗┓` or `╔╝╚╗╔╝╚╗` (a full frieze run)
- corner turn — `╔═╗ … ╚═╝` used as an L-fret where two rules meet

The meander is an **ornament seam**, so the Pantheon "one seam per container"
discipline applies to it too (§5): at most one meander frieze per surface.

### Entablature & pediment (the horizontal order + the cap)

A frame is read as a temple bay, top to bottom:

- **pediment** — a triangular header cap, `╱‾‾‾‾╲` (apex carries the surface's
  Greek order-mark, §1 letters); the acroterion.
- **cornice / architrave** — a heavy rule under the pediment, `═══════`
  (cornice) over a plain `───────` (architrave); the callout inscription sits
  on the architrave.
- **frieze** — the optional meander band between architrave and body.
- **stylobate** — the base rule the whole bay stands on: a stepped ground
  `▁▁▁▁` over `▔▔▔▔`, or a heavy `═══════` step.

### Greek-letter order-marks (semantic labels)

Capital Greek letters label surfaces the way the current lowercase callout ids
do — a stable per-surface **order-mark** carried at the pediment apex, WITH the
orchestration token preserved as the inscription beneath it (identity/search
survive). Assignment:

| order-mark | surface / callout token       |
|------------|-------------------------------|
| `Α`        | baton.control                 |
| `Β`        | terminals.roster              |
| `Γ`        | dag.trace                     |
| `Δ`        | meters.pulse                  |
| `Θ`        | power.reserve                 |
| `Λ`        | volume.level                  |
| `Ξ`        | battery.gauge                 |
| `Π`        | calendar.sheet                |
| `Σ`        | nowplaying.score              |
| `Ω`        | gadgets.case (the dock body)  |

Lowercase Greek (α β γ …) is available for sub-marks (e.g. a session's tier) but
is NOT required; the order-mark is the load-bearing use.

### The musical motifs coexist (unchanged)

The music glyphs are **inscriptions carved into the stone**, not replaced:

- the **state tier** `♪ 𝄐 𝄽 𝄂 ·` (theme.rs contract, §2) reads like a letter
  set into a frieze — it sits inside the body where it already sits.
- the **clef** `𝄞` (bar power key), the **rests** `𝄽 𝄾 𝄿 𝅀 𝅁 𝅂 𝆑`, the **note
  heads** `♩ ♪ ♫ ♬ 𝅘𝅥𝅮 𝅘𝅥𝅯`, the network marks `𝆹𝅥𝅮 𝆺𝅥𝅯`, and the seam run
  `𝄂𝄚𝅦𝄚` all stay verbatim.
- where a meander frieze and a musical seam would both appear, the musical seam
  wins (it carries meaning); the meander yields.

---

## 2. The glyph grammar — state tier is a HARD contract

Identical to Pantheon §2, restated so this grammar stands alone. The state
vocabulary is lifted VERBATIM from `pkgs/aoide/src/baton/theme.rs`
(`state_glyph`/`classify`) and kept in lockstep with the Rust — the grammar
**reuses** it, never replaces it:

| glyph | state                                   |
|-------|-----------------------------------------|
| `♪`   | working (running / active / tool)       |
| `𝄐`   | awaiting (await / block / notification) |
| `𝄽`   | idle                                    |
| `𝄂`   | done (also `stop`)                      |
| `·`   | unknown                                 |

Used by `BatonGadget`, `DagGraphGadget`, `TerminalManagerGadget`. The colours
partnering the glyph come from the roles (working → hot/accent, awaiting →
urgent, done → dim) exactly as today. No Greek motif touches these.

---

## 3. The role palette in the Greek key

Which Greek hue plays which `notes` role (full derivation in `design/intent.md`):

| role (notes)        | slot   | Greek hue           | where it works                          |
|---------------------|--------|---------------------|-----------------------------------------|
| `paletteBg`         | base00 | marble ground       | every glass body                        |
| `paletteFg`         | base05 | plum-charcoal ink   | all text, column rails at rest          |
| `paletteAccent`     | base0A | Attic gold          | active states, toggles, active workspace fill, running marks |
| `paletteUrgent`     | base08 | terracotta          | urgent / blocked                        |
| `paletteHot`        | base0B | laurel leaf-green   | the ONE blaze (traced element)          |
| `wireCyan`          | base0C | bronze-verdigris    | columns, meanders, structural rules     |
| `holoBlue`          | base0D | aegean deep-blue    | preview / information — the workspace preview ring, links, cool syntax ("the sea between the columns"); NOT chrome |
| `violet`            | base0E | Tyrian/murex        | DAG project steles                      |
| `glitchPink`        | base08 | terracotta          | urgent pulse (== urgent here)           |

**Restraint (the core rule, §5):** laurel leaf-green `hot` is the single traced
colour; everything else rests dim. The multicolour field is ROLES, not
decoration.

---

## 4. Widget-by-widget map

Each surface's function, data, and geometry are UNCHANGED. Only the drawn chrome
idiom changes. `GadgetFrame` is the main lever (it propagates to ~10 surfaces).

### `GadgetFrame` — the entablature (core; propagates to ~10 surfaces)

The pane becomes a **temple bay**. Element-by-element re-cast of the current
frame:

- **Depth stack retired.** Pantheon's two `holoBlue` offset outline copies (the
  3D "stacked offset volume") are DROPPED — this grammar is flat. `depthOff1/2`,
  `depthOpacity1/2`, `depthDx/Dy` go unused (or a single 1px `wireCyan` cast
  base-shadow under the stylobate replaces them — a column's ground shadow, not
  a 3D offset). No vanishing-point lean.
- **Callout row → pediment.** The lowercase-dotted title on its Canvas leader
  becomes a **pediment header**: a `╱‾‾‾‾╲` triangular cap with the surface's
  Greek order-mark (§1) at the apex, the orchestration token
  (`baton.control` …) inscribed on the architrave rule beneath it, in `labelColor`
  (= `paletteFg`) at the same dim opacity. The Canvas anchor-tick leader is
  replaced by the architrave rule `───────`.
- **Front outline → columns + cornice.** The single `wireCyan` hollow rule
  becomes: `║` column rails down the left and right body edges, a `═══════`
  cornice under the pediment, and a **stylobate** base rule (`▔▁` step) along the
  bottom. All `wireCyan` (bronze-verdigris), dim (~0.5), `radius: 0` kept.
- **Glass body + Aero gloss — UNCHANGED.** `glassColor` (= `paletteBg`),
  `glassOpacity` 0.72, and the gloss gradient stay exactly as they are.
- **`floatable` ↗ token** stays (affordance, not song), folded into the pediment
  raking-cornice tail.
- **`chromeDim` on trace — kept.** While a child gadget is traced, the pediment,
  columns, cornice, and stylobate step back to `chromeOpacity` (0.55) so the one
  laurel blaze in the body dominates. The glass body is left alone.

Because `BarPopout` and `DesktopGadgets` host `GadgetFrame`, they inherit the
entablature for free (no per-host headroom math is needed once the offset stack
is gone — a simplification the flat grammar buys).

### The dock — `AoideAgentWidgets` (the colonnade)

The dock is a **colonnade**: the container body is one long temple wall the
gadget bays stand inside.

- **Duplicated frame block must match `GadgetFrame`.** The dock body carries its
  own copy of the (now-retired) depth recipe — the two `holoBlue` offset copies
  at lines ~200–217. These are DROPPED in lockstep with `GadgetFrame`; the body
  keeps its `paletteBg` glass + `wireCyan` outline + gloss, re-cast as `║`
  column rails down both edges and a stylobate base rule.
- **Header callout → the dock pediment.** `gadgets.case` with its `pin.on/off`
  token becomes the `Ω` pediment (order-mark `Ω`, inscription `gadgets.case`);
  the pin token stays a lowercase-dotted state at the tail (kept verbatim —
  affordance).
- **Footer run — kept verbatim.** `𝄂𝄚𝅦𝄚𝄞𝅄ㅤ` stays as the dock's one musical
  seam, now read as the **stylobate inscription** carved along the base course.
  It wins over any meander frieze (§1).
- The gadget stack order (Baton, Terminals, DAG, slack, Meters, Power) and the
  hot-edge reveal/pin state machine are untouched.

### Dock gadgets (state glyphs + one-hot preserved)

- **`BatonGadget`** — the tab strip and SESSIONS mini-view keep their layout;
  the `𝄂𝄚𝅦𝄚` status-line prefix (its one seam) and the `♪𝄐𝄽𝄂` state glyphs are
  verbatim. Its empty-state note `nothing to conduct ♪(´ε｀ )` is kept. The
  only Greek touch: session rows may carry a lowercase `α/β/…` sub-mark, optional.
- **`TerminalManagerGadget`** — one row per session: `⠿` drag handle, agent,
  state glyph (`♪𝄐𝄽𝄂`), short cwd, elapsed. All kept. The **traced row** keeps
  its `paletteHot` (laurel) border + halo — the one-hot rule (§5). No 3D halo
  language change is needed; a border blaze is already flat.
- **`DagGraphGadget`** — the showpiece, re-cast as a **catalogue of steles**:
  - node boxes → small **steles/metopes**: single-ruled `wireCyan` box = session;
    double-ruled `violet` (amethyst) box = project (the double rule reads as a
    triglyph pair). Kept exactly as the current outline logic.
  - the drawn kinked-elbow **leader** (per-row Canvas) stays — it is a drawn
    line, not a wireframe volume, so it survives; recolour to `wireCyan` at rest,
    `paletteHot` (laurel) when the child is traced, as now. Optionally the elbow's
    corner turns a single meander fret, but the cheap straight elbow is fine.
  - the **traced node** keeps its `paletteHot` blaze: 2px laurel border + the
    two/three-ring halo (a flat glow, kept) + bold label. Still exactly ONE blaze.
  - state glyphs and lowercase callout ids (`session.9f3a…`) kept verbatim.
- **`MeterGadget`** — CPU/RAM `[▓▓▓░░░] NN%` ASCII gauges kept; the gauge bar
  reads as a fluted-column fill. Δ order-mark on its `GadgetFrame` pediment.
- **`PowerGadget`** — battery rest-notation icons (`𝄽 𝄾 𝄿 𝅀 𝅁 𝅂 𝆑`) + charge
  bar + network glyphs kept verbatim; its `𝄂𝄚𝅦𝄚` seam stays. Θ order-mark.

### The bar — `AoideBar` / `WorkspaceRow` (LIGHT TOUCH)

khoa: "the bar is ok, maybe add some greekness on top." Hard constraints kept:
the **36px strip geometry**, the **black structural staff ink** (`#000000` staff
lines, barlines, playhead), and the **functional glyph set** (clef `𝄞`, rests,
note-heads, network `𝆹𝅥𝅮/𝆺𝅥𝅯`) — all untouched. The cream `paletteBg` @0.45
glass and Aero gloss stay. Only additive Greek seasoning, all in black ink so it
reads as part of the engraved staff:

- a single small **meander fret** (`⌐¬` or `┏┛┗┓`, ~2–3 cells) sits just after
  the clef as a key-signature ornament, before the first staff content — the one
  Greek mark on the strip.
- the final `𝄂` measure-close may gain a matching tiny meander tick to bookend
  it. Optional; symmetric with the head fret.
- the clock/title `/` slur may render as a Greek middot `·` (already black,
  already dim) — a one-glyph nod, no geometry change.
- **`WorkspaceRow` untouched.** Note-heads, pitch-on-staff, the accent-fill
  active swell, the `holoBlue` preview ring, the `glitchPink` (terracotta) urgent
  pulse — all kept. The melody is not Greek-ified.

### Flat skeletons

Each is a plain `Rectangle`/`Item` with a `border`; the Greek grammar gives each
a **stele** treatment (pediment cap + column rails + stylobate base), drawn in
box characters, function identical:

- **`NotificationCard`** — a **votive stele**: a `╱‾‾╲` pediment carrying the app
  name, `║` rails, a stylobate base rule. Urgent (`urgency === 2`) swaps the rail
  colour to `notifUrgent` (terracotta) and pulses — the existing
  `notes.notifUrgent`/`windowBorder` border logic, re-dressed as rails.
- **`AoideOsd`** — a small centred **stele**: pediment + a single value line;
  border `paletteAccent` (now gold) kept. The fade-in/out timing is untouched.
- **`AoideLockscreen`** / **`AoideGreeter`** — a **temple façade**: a wide
  `╱‾‾‾‾‾╲` pediment over a `‖ ‖ ‖` colonnade framing the password/login field;
  border `paletteAccent` kept. Pure chrome dress over the existing skeleton.
- **`AoideSessionGraph`** / **`GraphRow`** — the overlay DAG mirrors
  `DagGraphGadget`'s stele idiom: project rows `◆`→ double-ruled amethyst stele,
  session rows `●`→ single `wireCyan` stele + state glyph + short cwd; depth via
  indentation kept; `stateColor` map (`paletteAccent`/`paletteUrgent`/dim) kept.
  Clicking a session row still routes `activate(windowAddress)` through the gate.

---

## 5. The one-hot rule & restraint in the Greek key

Mirrors Pantheon §4–5's discipline, transposed to marble:

- **One blaze.** Exactly the traced/live element blazes **laurel green**
  (`paletteHot`, base0B) — the DAG traced node (2px laurel border + halo + bold
  label) and its partner **TERMINALS** traced row (border + halo). Nothing else
  is ever laurel. When nothing is traced, the whole field rests dim.
- **The field rests low.** Project steles read `violet` (Tyrian/murex) a touch
  stronger than idle session steles (`wireCyan`); done nodes nearly vanish (dim
  ~0.2). The dim ladder is unchanged from the Pantheon constants (`dimProject`
  0.45 · `dimIdle` 0.35 · `dimDone` 0.2 · `dimCallout` 0.35).
- **INVARIANT — the verdigris cap.** All structural `wireCyan` (bronze-verdigris)
  chrome — columns (`║ ▌ ▐ │ ‖`), meander friezes, cornices, and stylobates —
  MUST render at **≤ 0.5 alpha at all times**. This is a hard rule the reskin
  must honor, not a tunable default. On the warm marble ground the teal verdigris
  is chromatically louder than the laurel `hot` blaze, so the opacity ladder is
  the ONLY thing keeping the structural role from out-blazing the one-hot green.
  A verdigris rule drawn at full strength would steal the trace. Never exceed 0.5.
- **Three jobs, three hues, never crossed.** **Gold = the chrome state**
  (`paletteAccent`, base0A — active workspace fill, open toggles, running marks);
  calm and everywhere-eligible. **Laurel = the one blaze** (`paletteHot`, base0B —
  the traced element only). **Terracotta = the summons** (`urgent`/`glitchPink`,
  base08 — blocked pulse, low battery). Aegean (`holoBlue`, base0D) is a fourth,
  quieter voice — preview/information (the workspace preview ring, links), never
  chrome state and never a blaze.
- **STANDING RULE — gold is a fill / line, never running body text.** Attic gold
  `paletteAccent` clears only 3.54:1 on marble (< 4.5 AA), so it may fill a
  note-head, stroke an active border, or tint a toggle, but MUST NOT be used for
  running body text. Same class as the verdigris cap: a chrome colour with a
  contrast ceiling, not a text colour. (The verdigris ≤ 0.5 invariant above is
  unaffected and still stands.)
- **Colours are ROLES, not ornament.** The meander and columns do not introduce a
  second decorative hue; they wear the structural `wireCyan` role. A surface that
  reaches for a stray extra colour has left the grammar.

---

## 6. What the grammar excludes

- **No 3D wireframe.** The offset outline-copy depth stack (Pantheon §1) and the
  vanishing-point lean are absent — sonata's temple is drawn flat. A stray offset
  ghost reads as a surface that missed this grammar.
- **No gratuitous ornament.** A meander or column with no structural job is out,
  exactly as Pantheon excludes decorative end-caps. One meander frieze per
  surface at most; the musical seam outranks it.
- **No re-lettering of the music contract.** The `♪ 𝄐 𝄽 𝄂 ·` state tier, the
  clef/rest/note functional glyphs, and the `𝄂𝄚𝅦𝄚` seam are never swapped for
  Greek forms — they are the Rust-locked score, and the Greek forms frame them,
  never replace them.
