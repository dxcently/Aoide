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
`holoBlue`, `violet`, `glitchPink`, `windowBorder`, `barBg`) and keeps the same
real-data discipline (FileView seams, real Quickshell services, no invented
IPC), the same 36px bar strip, and the same click targets. It began as a
chrome-only reskin plan; the **pantheon rebuild** (`353390a`) then rebuilt the
dock widgets FRESH in this grammar — §4 records the as-built result, not the
original reskin mapping. The **musical state-glyph contract is untouched
throughout** — `♪ 𝄐 𝄽 𝄂 ·` stay verbatim (see §2).

---

## 1. The motif vocabulary

Five architectural motifs, drawn from monospace text — or, since the pantheon
rebuild, as flat **Canvas line-work** (1–1.5px strokes: still a drawn line,
never a rendered volume). The temples paint their ornament courses this way —
the Greek-key meander, egg-and-dart, the triglyph-and-metope band, the Ionic
volute capital + dentil course — while frames, rails, and stylobates stay box
characters. They are the alphabet every surface is built from.

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
| `Α`        | conductor.control              |
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

The full table lives verbatim in `GadgetFrame`'s `orderMarks` map, but after
the pantheon rebuild only the bar-popout marks are LIVE on screen — `Λ` `Ξ`
`Π` `Σ` (plus the `·` fallback the wallpaper picker's `wallpaper.summon`
falls to). `Α Β Γ Δ Θ Ω` have no surface carrying their titles any more: the
temples are self-framed and inscribe their names in carved serif
(CONDUCTOR · TERMINALS · METERS · POWER) instead of a pediment order-mark
(§4).

### The musical motifs coexist (unchanged)

The music glyphs are **inscriptions carved into the stone**, not replaced:

- the **state tier** `♪ 𝄐 𝄽 𝄂 ·` (theme.rs contract, §2) reads like a letter
  set into a frieze — it sits inside the body where it already sits.
- the **clef** `𝄞` (bar power key), the **rests** `𝄽 𝄾 𝄿 𝅀 𝅁 𝅂 𝆑`, the **note
  heads** `♩ ♪ ♫ ♬ 𝅘𝅥𝅮 𝅘𝅥𝅯`, and the network marks `𝆹𝅥𝅮 𝆺𝅥𝅯` all stay verbatim.
  (The old dock's `𝄂𝄚𝅦𝄚` seam run retired with the colonnade — no surface
  draws it today.)
- each temple carries its OWN clef as its crown (§4): `𝄞` treble (Conductor),
  `𝄢` bass (Terminals), `𝄡` alto (Meters) — and Power crowns with the Greek
  koppa `ϟ`, a letter that reads as Zeus's lightning, standing in where no
  clef fits.
- where a meander frieze and a musical seam would both appear, the musical seam
  wins (it carries meaning); the meander yields.

---

## 2. The glyph grammar — state tier is a HARD contract

Identical to Pantheon §2, restated so this grammar stands alone. The state
vocabulary is lifted VERBATIM from `pkgs/aoide/src/conductor/theme.rs`
(`state_glyph`/`classify`) and kept in lockstep with the Rust — the grammar
**reuses** it, never replaces it:

| glyph | state                                   |
|-------|-----------------------------------------|
| `♪`   | working (running / active / tool)       |
| `𝄐`   | awaiting (await / block / notification) |
| `𝄼`   | stopped (the turn just ended, < 1h ago) |
| `𝄽`   | idle (cold: stopped > 1h, or fresh/resumed) |
| `𝄂`   | done (the session itself ended)         |
| `·`   | unknown                                 |

Used by `ConductorGadget` and `TerminalsGadget` (both switch on the daemon's
canonical `working|awaiting|stopped|idle|done` strings — no regex derivation).
`stopped` is a genuinely separate fifth state, not a `done` alias — the bridge
splits "the turn ended" (stopped, decaying to idle after an hour of no
activity) from "the session ended" (done). The colours partnering the glyph
are a fixed shared spread: working → gold (`paletteAccent`), awaiting →
terracotta (`paletteUrgent`), stopped → aegean (`holoBlue`), idle → murex
(`violet`), done → verdigris (`wireCyan`), unknown → dim ink — identical in
every temple, so a "working" note reads the same in every house. The
emphasized/traced row overrides its glyph to laurel (§5). No Greek motif
touches the glyphs themselves.

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
| `holoBlue`          | base0D | aegean deep-blue    | preview / information — the workspace preview ring, links, cool syntax ("the sea between the columns"); NOT chrome. Also the `stopped` state colour (§2) — a second job, not a chrome exception |
| `violet`            | base0E | Tyrian/murex        | DAG project steles                      |
| `glitchPink`        | base08 | terracotta          | urgent pulse (== urgent here)           |

**Restraint (the core rule, §5):** laurel leaf-green `hot` is the single traced
colour; everything else rests dim. The multicolour field is ROLES, not
decoration.

---

## 4. Widget-by-widget map

AS BUILT after the pantheon rebuild (`353390a feat(rice/sonata): Greek pantheon
widgets` and the bridge-view passes that followed it). The old dock family —
`AoideAgentWidgets`, `BatonGadget`, `TerminalManagerGadget`, `DagGraphGadget`,
`MeterGadget`, `PowerGadget`, `DesktopGadgets`, `AoideSessionGraph`,
`GraphModel`/`GraphRow`, `ClockGadget`, `SessionChip` — was DELETED, not
reskinned. The field now splits into two chrome families:

- the **entablature** — `GadgetFrame`, the box-character temple bay, worn by
  the bar's popouts (and the wallpaper picker);
- the **pantheon temples** — four SELF-FRAMED marble steles (`ConductorGadget`,
  `TerminalsGadget`, `MetersGadget`, `PowerVitalsGadget`) stacked inside the
  codex dock (`AoidePanel`). These are fresh builds in this grammar, each
  drawing its own chrome; none of them instantiates `GadgetFrame`.

**Amendment (khoa, 2026-07-31) — the two families converge on ONE panel
chrome.** `GadgetFrame`'s flat entablature bay is narrowed/superseded as the
default for POPOUT BODIES: every widget's outer panel — bar popouts included,
not just dock temples — should read as the same opaque marble-stele grammar
`NotificationCard.qml` and the four pantheon temples already share (opaque
body, 2px `paletteFg` border, 1px inset signature-hue keyline, cast shadow, a
box-drawing TUI frame top AND bottom closing on a `𝄂` barline, a carved-serif
crown + name caption, `radius: 0`) — see `NotificationCard.qml`'s own header
for the canonical description. `GadgetFrame` may still own the outer
`BarPopout` window/positioning plumbing; the CONTENT chrome inside it
converges on the stele, not the flat bay. khoa's framing: **"it's ascii/
greek/diff pantheon styles for each widget"** — ONE shared body-chrome
convention, but every widget keeps its own distinct classical-order identity
(hue/crown/motif/frieze) within it, same as the four temples already do
relative to each other. Applies to every widget built from this point
forward (the audio/mic+vol widget and the calendar widget are the first
carrying it).

**Amendment (khoa, 2026-07-31) — off/muted/disabled states read as literally
"broken."** Where a widget has an off/muted/disabled state to represent (the
audio widget's mic/vol mute is the first case), reach for literal broken/
ruined-classical-architecture iconography — a snapped column shaft, a missing
capital, a jagged break — legible from silhouette alone, before reaching for
an abstract overlay (a slash-through, a dim badge, a strikethrough). This
sits alongside, not instead of, the §2 session-state glyph contract above —
that table stays the hard contract for agent/session state; this is the
parallel convention for a widget's own on/off controls.

### `GadgetFrame` — the entablature (bar popouts + wallpaper picker)

The pane is a **temple bay**, implemented as designed:

- **Depth stack retired.** The two `holoBlue` offset outline copies are gone —
  drawn nowhere. `depthOff1/2`, `depthOpacity1/2`, `depthDx/Dy` remain as no-op
  property declarations (hosts still set them), and `depthExtent` is pinned 0.
- **Pediment header.** A `╱‾‾ Α ‾‾╲` raking cap carries the surface's Greek
  order-mark (§1) at the apex; a heavy `═` cornice runs beneath it; the
  orchestration token is inscribed after a short `───` architrave tick in
  `labelColor` (= `paletteFg`), dim 0.55. The Canvas anchor-tick leader is gone.
- **Columns + stylobate.** The side rails are crisp 1px verdigris rules (a tiled
  `║` glyph broke up at variable bay heights — legibility-first, the shaft is a
  rule); the base is a `▔` over `▁` stepped stylobate in text. All `wireCyan`
  at 0.5 alpha (the §5 cap), `radius: 0`.
- **Glass body + Aero gloss — kept.** `glassColor` (= `paletteBg`),
  `glassOpacity` 0.72, and the gloss gradient are unchanged.
- **`floatable` ↗ token** kept, folded into the pediment tail.
- **`chromeDim` on trace — kept.** Chrome steps to 0.55 while a body child's
  `shared.tracedSessionId` is live; the glass body is left alone.

**Hosts, as wired today:** the bar's four `BarPopout`s — `volume.level` (`Λ`),
`battery.gauge` (`Ξ`), `nowplaying.score` (`Σ`), `calendar.sheet` (`Π`) — which
override the chrome to the popout palette (cream card at 0.72 over
`blur_popups`, plum-ink rails and label), and `AoideWallpaperPicker`
(`wallpaper.summon`, no assigned order-mark → the `·` fallback). The dock does
NOT host `GadgetFrame` any more; the `Α Β Γ Δ Θ Ω` entries in its order-mark
map still exist in code but no live surface carries those titles.

### The dock — `AoidePanel` (the codex)

The colonnade is gone; the dock was rebuilt as a **codex** — a closed book seen
edge-on, peeking from the CENTER-LEFT screen edge (`SUPER+G`, the `aoide:dock`
global shortcut). Not a `GadgetFrame` host and not glass: an OPAQUE marble
board (`paletteBg`) with the hard 2px plum border + inset 1px Attic-gold
keyline + cast shadow the temples share.

- **Spine** (inner edge) — a carved band: twin gold rules + a column of `◆`
  binding stations.
- **Fore-edge** (outer, screen-facing edge) — a stack of page-edge striations:
  thin varied `paletteFg` rules with an occasional gilt (`paletteAccent`) leaf
  and a gold ribbon bookmark. This sliver is what peeks at rest.
- **Header** — a `𝄞` clef cartouche + "AOIDE" in carved serif, answered by a
  small italic `ᾠδή` on the right, over a hairline gold rule.
- **Body** — the four temples stacked at native size (Conductor 360×520,
  Terminals 360×520, Meters 340×268, Power 340×268) in a Flickable capped at
  ≤ 92% of the screen; a slim gold scrollbar in the gutter and a breathing
  `▽ more` hint when content is below the fold.
- **The awaiting peek** — the dock stays hidden until an agent needs a
  response: any session in canonical state `awaiting` slides the fore-edge
  sliver out as an alert (debounced against heartbeat rewrites of
  `sessions.json`). Opening the dock — keybind, hot-edge hover, or a click on
  the sliver — acknowledges it; the peek re-arms on the next false→true edge.

The old `Ω`/`gadgets.case` pediment, the pin token, and the `𝄂𝄚𝅦𝄚𝄞𝅄` footer
seam went down with the colonnade. The fold-out journal presentation
(`AoideJournal.qml`) — a duplicate fork of the dock built at the same commit
as the codex but never instantiated by `shell.qml` and never carried forward
past that one commit — has been DELETED, same as `DagGraphGadget` below; an
open flag on reviving its "fold-out book" idea, if wanted, is tracked outside
this file (see the dev handoff).

### The four temples — the shared pantheon bars

Each dock gadget is a distinct **temple**: same family, different god's house.
Every temple carries: the opaque marble stele (2px plum border, inset 1px
keyline in its signature hue, cast shadow, `radius: 0`) — Terminals alone keeps
a translucent 0.72 glass body (compositor blur + hyprglass frost through it);
a carved-serif inscription beside its clef; a Canvas-drawn frieze in its
signature hue; a box-drawing TUI frame (`┌─┤ label ├…┐` over the body,
`└─┤ tally ├ … 𝄂 ┘` closing it — the final barline closes every score); the
`♪ 𝄐 𝄽 𝄂 ·` state tier verbatim (§2); kaomoji mood faces; a `TACET` +
`ᕕ( ᐛ )ᕗ` empty state (Conductor's under an ASCII temple, Terminals' under an
ASCII column); gold ink for the numeric tallies; and exactly ONE laurel
`paletteHot` standout (§5). Type is three voices: Noto Serif (carved marble),
JetBrainsMono (the terminal), Noto Music (notation).

| temple | order | signature hue | clef / crown | frieze |
|---|---|---|---|---|
| `ConductorGadget` | Doric | Attic gold (`paletteAccent`) | `𝄞` treble | Greek-key meander |
| `TerminalsGadget` | Ionic | aegean (`holoBlue`) | `𝄢` bass | volute capital + dentil course + egg-and-dart |
| `MetersGadget` | Doric | verdigris teal (`wireCyan`) | `𝄡` alto C | triglyph-and-metope |
| `PowerVitalsGadget` | Corinthian | murex (`violet`) | `ϟ` koppa (a Greek letter as lightning — no clef) | egg-and-dart ovolo |

The state COLOUR spread is shared across temples: working → gold
(`paletteAccent`), awaiting → terracotta (`paletteUrgent`), idle → murex
(`violet`), done → verdigris (`wireCyan`), unknown → dim ink. (This supersedes
the old "done → dim" pairing; see §2.)

- **`ConductorGadget`** — the agent roster (`[ conductor ]`): only what a conductor
  conducts — agent/subagent sessions, plus shells sharing an agent's window,
  read as a pure view of the daemon's `sessions.json`. Rows are a **stave**:
  each TOP-LEVEL session a note on a ledger line, hung on a `│` pilaster
  tinted by its per-agent identity hue (the base16 `noteColor` spread; a
  conducted shell borrows its conductor's hue). Child sessions are **beamed**
  off their parent — a stem + quaver-beam limb in the parent's hue, running
  all the way to the child's name; the child carries NO notehead of its own,
  depth clamped to one indent (deeper nesting flattens to the same limb).
  State is spoken by the glyph/colour pair (§2) and by the kaomoji layer
  below — the note glyph itself no longer bobs, and there is no shimmer or
  idle-breath animation on the ledger; that motion moved entirely onto the
  face. The awaiting row wash + a deep held note-pulse are kept. Rows carry
  name (session title, else agent) with its state tag on the SAME line, then
  a second tally line below: elapsed time, the agent/subagent tag (`⟐` marks
  a subagent), a `⇢ conductedBy` tag where relevant, and the `ws N` workspace
  tag — followed by a `▸` activity line, a quoted "say" line (the agent's
  latest transcript words), short cwd in aegean, and the kaomoji riding the
  cwd line at a fixed right-aligned width. Hovering a row previews its
  workspace on the bar; click → `bridge.focusSession`. The emphasized row
  (traced, else first working) takes the laurel: 3px spine + wash + laurel
  note/elapsed.
- **`TerminalsGadget`** — the tty roster (`[ tty ]`): EVERY live terminal
  window, tracked or not (the daemon publishes synthetic `shell` records;
  the widget only filters + de-dupes by window address — agent record wins a
  shared window). Rows hang on a `║` twin-groove column; the main label is the
  running process/command, with the SAME second-tally-line layout as the
  Conductor (elapsed + `ws N`, no agent/conductor tags — this roster has no
  tree) below it; the cwd rides on its own line in aegean; scroll-cornered
  frame (`╭ ╮ ╰ ╯`, echoing the volutes). Same hover-preview, click-to-focus,
  say-line, kaomoji-on-the-cwd-line, and laurel-crown row idioms as the
  Conductor — only the architecture differs.

**The kaomoji layer** (`MoodFaces.qml`, shared verbatim by both temples,
instantiated as a plain child object — it holds no state, so nothing needs
threading from `shell.qml`) is the state's expressive voice now that the
ledger itself sits still. It has TWO pools:

- **`working`** — the general pool (currently 15 sets), a lively unthemed
  vocabulary (a cheer, a dance, a table flip, a stroll-whistle, a fishing
  cast…) with no forced concept and no forced facing direction; two design
  passes tried both a music-desk theme and a mandatory left-facing rule and
  both were explicitly retired. Every top-level agent row and every terminal
  row draws from this pool — and the pick is a genuine `Math.random()`, not a
  hash: it re-rolls every time the row's delegate is (re)created, i.e. on
  every roster refresh. Deliberate: variety over identity for these rows.
- **`packages`** — the subagent pool (currently 4 courier sets: `haul`,
  `handoff`, `stack`, `roll`), each carrying the parcel glyph `口`. Only rows
  where `kind === "subagent"` ever draw from it. Unlike the general pool, a
  subagent's set is HASHED from its session id and held for its whole life —
  a courier's small delivery story reads better with a consistent identity
  than with the general pool's per-refresh shuffle.

Both pools follow the same authoring rules regardless of theme: motion must
be POSITIONAL (a prop travelling a fixed track, or the whole figure changing
cell), every frame within a set renders at an identical width (fixed-width,
right-aligned box — a wider frame just shifts its own left edge and reads as
a stretch), padding that needs to hold width uses the ideographic space
U+3000 (an ASCII trailing space is trimmed by Qt and won't shift anything),
and glyphs stay inside the shell's proven CJK/kana/music repertoire (tofu is
constant-width and reads as a working animation that quietly died). Each
resting state (`awaiting`/`stopped`/`idle`/`done`/unknown) holds one still
pose, shared by both pools.
- **`MetersGadget`** — CPU + RAM (`[ /proc ]`), read from `/proc/stat` +
  `/proc/meminfo` on a ~2s tick. Each meter is a **voice**: its load spoken as
  a dynamic marking (`𝆏𝆏 𝆏 𝆐 𝆑 𝆑𝆑`), its gauge an ASCII bar `⟦▓▓▓░░░⟧` — CPU
  fills murex, RAM fills aegean, ≥ 85% goes terracotta. One kaomoji reads the
  whole box; honest bytes (`used / total GB`) sit on the closing ledger line.
  The laurel goes to the CALMEST voice, and only if genuinely calm (< 55%) —
  silence is the healthy note.
- **`PowerVitalsGadget`** — battery + network (`[ upower ]`), from UPower and
  `/proc/net/route`. The battery drains toward silence: charge is a rest glyph
  (`𝄽 𝄾 𝄿 𝅀 𝅁 𝅂` emptier→fuller, full `𝆑`, charging `𝄮`, mains `𝄻`) over an
  ASCII charge bar (teal fill; gold when charging/full; terracotta when low).
  The link is a sustained note `𝅗𝅥` while up, a rest `𝄽` when dead; iface +
  detail in aegean. An HONEST empty state on a desktop: "AC — no battery
  present", never a faked 100%. The laurel goes to the live network link.

### The preview harnesses — `*Preview.qml`

Each temple has a **preview sibling** (`ConductorPreview`, `TerminalsPreview`,
`MetersPreview`, `PowerPreview`): a standalone `qs -p` harness that floats just
that gadget on an overlay surface for a screenshot, carrying a stub gold-marble
palette (hex values sanctioned here only — they mirror the livery roles) and
a stub bridge. The
roster pair honour `QS_STAGE` to exercise the empty state; Meters/Power read
the machine's real `/proc` + UPower live. They are development fixtures, not
shell surfaces — `shell.qml` never loads them.

### `DagGraphGadget` — RETIRED

The stele-catalogue DAG showpiece was deleted in the rebuild along with its
overlay mirror (`AoideSessionGraph`/`GraphModel`/`GraphRow`). Nothing replaces
it in this grammar: the Conductor's beamed tree is the only nesting view. The
`Γ`/`dag.trace` order-mark survives unused in `GadgetFrame`'s map, and the
`aoide.surfaces.sessionGraph` registry entry has no QML body (an open flag,
tracked outside this file).

### The launcher — `AoideLauncher` (the Propylaea)

The one GLASS temple among the opaque steles: the launcher is the entrance —
the propylaea, the gate you pass through to summon an app — so its body stays
frosted (`paletteBg` translucent over compositor blur + hyprglass) while
wearing the full pantheon chrome: 2px plum border, inset gold keyline,
`radius: 0`, a carved-serif inscription, a Greek-key meander rule, a `┤ … ├`
TUI frame around the search line, the `♪` prompt idiom, kaomoji on the empty
stage. The selected row is its one laurel: a `paletteHot` note + spine over an
Attic-gold accent box.

### The bar — `AoideBar` / `WorkspaceRow` (what actually shipped)

khoa: "the bar is ok, maybe add some greekness on top" — the planned meander
fret / `𝄂` bookend tick / middot slur were NEVER built; no Greek mark sits on
the strip. What shipped instead is the manuscript strip going OPAQUE: the sheet
fills `paletteBg` at alpha 1.0, no glass, no gloss (intent.md's surface table
is the record). Kept: the 36px geometry, the black structural staff ink
(`#000000` staff lines, barlines), and the functional glyph set (`𝄞` clef —
also the powermenu key, rests, note-heads, network marks, the closing `𝄂`).
The `✎N` agent-sessions cell rests laurel `paletteHot` and pulses `glitchPink`
when a session blocks. `WorkspaceRow` did change, in the SONG vein, not the
Greek: each workspace is a solid note glyph carrying its OWN hue from the
base16 `noteColor` spread; the selected note keeps its own hue but swells,
rests on a same-hue highlight pill, and takes a white outline (the one
sanctioned colour literal, for separation on the opaque sheet); hover-select
previews in gold; the widget-hover preview ring (holoBlue) marks whichever
workspace a hovered Terminals row lives on; urgent pulses `glitchPink`. All
THREE of these marks (`highlight`, `previewRing`, the per-cell hover pill) are
genuinely round (`radius: width / 2`) — a deliberate, narrow exception to the
shell's otherwise-universal `radius: 0` hard-corner rule (§4 intro, §5): they
were always called "pills" in the code and the name is now literal, since a
square-cornered "pill" never actually read as one. Nothing else in the shell
rounds a corner. The melody is colour-coded, not Greek-ified.

### Flat skeletons — RETIRED 2026-07-30

`NotificationCard`, `AoideOsd`, `AoideLockscreen`, `AoideGreeter` (plus
`AoideNotifications`, `CalendarGadget`, `NowPlayingGadget`, none of which had a
grammar entry here) were **deleted** from the shared `qml/` tree — they were
song-blind stubs/chrome with no per-song identity, cleared to make room for
§7's staging engine. Their old stele/temple-façade treatment
(pediment + rails + stylobate, described here until this edit) is no longer
live anywhere; a song authoring a replacement under the §7 convention starts
fresh, it doesn't need to match this retired look.

---

## 5. The one-hot rule & restraint in the Greek key

Mirrors Pantheon §4–5's discipline, transposed to marble — restated to what the
pantheon rebuild actually enforces:

- **One laurel standout PER TEMPLE.** Laurel green (`paletteHot`, base0B) is
  still the only standout hue, but the unit of restraint is the temple, not the
  whole desktop: each surface elects exactly one laurel element. Conductor and
  Terminals crown the traced row (`shared.tracedSessionId`, else the first
  working row) — 3px laurel spine + wash + laurel note/elapsed; Meters crowns
  the CALMEST voice, and only if genuinely calm (< 55%); Power crowns the live
  network link; the launcher crowns the selected row; the bar's `✎N` cell
  rests laurel. Nothing else is ever laurel. (Note the drift from the original
  "exactly one blaze in the whole field" — several temples can each show their
  one standout at once.)
- **The field rests low.** Idle rows and empty states read dim ink
  (0.4–0.65 washes); ledger lines run their temple's hue at ~0.3; the
  emphasis/hover washes are 0.08–0.10. The old DAG dim-ladder constants
  (`dimProject`/`dimIdle`/`dimDone`/`dimCallout`) retired with the DAG.
- **The verdigris cap — narrowed to the STRUCTURAL role.** Where `wireCyan`
  plays structural chrome — `GadgetFrame`'s rails/cornice/stylobate, the
  skeletons' rails and colonnades, ledger lines — it holds the original
  **≤ 0.5 alpha** cap (the teal would out-shout the laurel at full strength).
  BUT the Meters temple wears teal as its SIGNATURE hue, and signature chrome
  (clef, keyline, frieze, TUI frame) runs strong (0.55–0.95, the frieze at
  full stroke) in every temple by design — Meters' teal included. As built,
  the cap binds the role, not the hue. (This is a departure from the original
  Fable-review invariant as worded in intent.md's log; flagged, not silently
  blessed.)
- **Hues now hold several jobs each.** The rebuild added two job classes on
  top of the original three: the per-temple **signature hue** (gold /
  aegean / teal / murex carrying one temple's architecture each, §4) and the
  shared **state spread** (working gold · awaiting terracotta · idle murex ·
  done verdigris, §2), plus gauge fills (CPU murex, RAM aegean, battery teal)
  and the per-agent/per-workspace **identity cycle** (`noteColor` over the
  8-slot base16 spread). Laurel alone keeps a single job — the standout.
  Terracotta remains the summons everywhere (awaiting pulse, low battery,
  urgent workspace, urgent notification).
- **STANDING RULE — gold is a fill / line, never running body text.** Attic gold
  `paletteAccent` clears only 3.54:1 on marble (< 4.5 AA), so it may fill a
  note-head, stroke an active border, tint a toggle — and, in the temples, ink
  the short numeric tallies (elapsed, percentages) — but MUST NOT carry running
  body prose. Body text is always `paletteFg` ink.
- **Colours are ROLES, not ornament.** An ornament course wears its temple's
  signature hue and nothing else; a surface that reaches for a stray extra
  colour beyond its signature + the shared spreads has left the grammar. The
  one sanctioned literal in the whole shell is the white outline on the bar's
  selected note (§4).

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

## 7. Per-song flavor widgets — BUILT for `calendar` + `notifications`

**Status: BUILT (2026-07-30, plan→execute pipeline), for exactly two slots.**
khoa's original framing held: the rerice mechanic is declarative and mostly
nix, and a song carries its OWN QML for "flavor" surfaces while core chrome
(bar, launcher, wallpaper, dock frame + gadgets) stays one shared, song-blind
implementation. Live hot-swap (via `aoide rice preview <name>`, no rebuild)
works for these too, not just colours — confirmed live: the bar's calendar
popout swaps body between `default` and `sonata` with no rebuild/restart.

The six-slot sketch this section originally carried was YAGNI-trimmed to the
two slots that actually have a host anchor. `greeter`/`lockscreen`/`osd`/
`nowPlaying` remain **unbuilt, no host anchor** — no live surface exists to
hang them off yet; they stay a documented future extension point, not
speculative code.

**As-built shape:**
- **Convention:** `song/songbook/<name>/widgets/<slot>.qml`, fixed slot enum
  **`{ calendar, notifications }`**. A song omits files for slots it doesn't
  dress.
- **Build:** the quickshell facet's derivation
  (`modules/facets/quickshell/default.nix`), after its existing `cp -r`, walks
  `song/songbook/*/widgets/` and copies each in-scope slot file to
  `$out/qml/songs/<name>/<slot>.qml`, plus a generated
  `$out/qml/songs/manifest.json` (`{ "<name>": ["<slot>", …] }`) — ALL songs'
  bodies land on disk at once, which is what makes cross-song live preview
  possible at all.
- **Runtime resolution:** `LiveryState.qml`'s `songName` property reads an
  **additive** `song` field `aoide rice preview <name>` now injects into
  staged `livery.json` (CONTRACTS.md §4 — no schema version bump; the same
  additive precedent as `parentSessionId`). The staging engine (`StagingEngine.qml`) reads the
  manifest (`has`/`source`); `WidgetSlot.qml` is the fixed per-slot anchor —
  it resolves the song's file when authored, else falls back to shared chrome
  (or nothing, for `calendar`, since the old song-blind `CalendarGadget` was
  retired). `WidgetSlot` manages the loaded item's lifecycle via
  `Component.createObject(parent, initialProperties)` rather than a
  declarative `Loader` — QML `required property` can only be satisfied at
  object creation, which a `Loader`'s `onLoaded`-time property assignment is
  too late for.
- **Live hosts:** `AoideBar.qml`'s calendar `BarPopout` (click the clock
  cell) and `AoideNotifications.qml`'s per-card `Repeater` (falls back to the
  shared `NotificationCard` — every card renders via that fallback this pass,
  since no song has authored `widgets/notifications.qml` yet).
- **Containment:** widget QML is store-copied score, like cover art — at
  runtime it sees only `LiveryState` (`notes`) + `ShellBridge` (`bridge`),
  plus a slot's declared extras (`notifications`' `notification`), never nix
  `config.*` — a song stays structurally incapable of leaking host/facet
  options through this surface (CONTRACTS.md §5 holds, unchanged: a song's
  `rice.nix` still sets **only** `aoide.livery`).
- **Touched:** `modules/facets/quickshell/default.nix`,
  `pkgs/aoide/src/dispatch.rs` (`handle_rice_preview`),
  `LiveryState.qml` (new `songName`), the staging engine (`StagingEngine.qml` + `WidgetSlot.qml`)
  (new), `AoideBar.qml`, `AoideNotifications.qml`, `shell.qml`,
  `song/songbook/{default,sonata}/widgets/calendar.qml` (new proof stubs),
  `song/songbook/update-playbook.md` (new). **Confirmed not needed** (as
  predicted): `lib/checks.nix` (`songShape` only inspects `.nix` files;
  `no-song-read` only bans runtime dirs, not committed `songbook/`); no
  `modules/nucleus/options.nix` submodule (widgets are files, not nix
  options); no `CONTRACTS.md` schema version bump (the `song` field is
  additive, same as `parentSessionId`).

See `docs/Aoide-Wiki/references/AOIDE-DEV-HANDOFF.md` §7 for the ledger flag.
