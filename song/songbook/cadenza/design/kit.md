# cadenza kit — the shared drawing vocabulary

The kit is how every cadenza widget draws the grammar of `intent.md` §2: one
character cell, one set of colour roles, one pane, a small set of
instruments. It lives in `widgets/` next to the slots that use it:

| file | kind | job |
|---|---|---|
| `Kit.js` | JS library | colour roles, the cell, the icon set, glyph builders, helper URLs |
| `Pane.qml` | helper | the termui pane: hairline rules, title and stat in the top rule, inner glow, reveal/close |
| `Block.qml` | helper | the borderless block: one message that is not a pane |
| `GlowText.qml` | helper | text with glow tier 0 (outline) or tier 0+1 (baked bloom) |
| `Gauge.qml` | helper | eighth-block gauge on a `░` track, % right-aligned |
| `Sparkline.qml` | helper | `▁…█`, one column per sample |
| `BrailleChart.qml` | helper | U+2800 line chart, 2×4 dots per cell |
| `BarChart.qml` | helper | full-block columns, value and label under each |
| `StateLamp.qml` | helper | `● ◐ ○ ■ ✕` in its state colour |
| `Specimen.qml` | specimen | every piece on one canvas (`design/shots/specimen-offscreen.png`) |

A helper file is uppercase, so it is never a slot. It is never used by type
name. That rule is §1.

## 1. Sharing: by URL, never by type name

**The gap.** Quickshell loads a song's files through its `qs:@/qs/` scheme.
Qt resolves a same-directory type such as `Pane { }` implicitly only for
local directories. For a remote scheme like `qs:` it needs a `qmldir` that
names the type. Without one it fails with `Pane is not a type` (unless the
type happens to be in the component cache already). The nix build writes
that qmldir for every uppercase helper
(`modules/facets/quickshell/default.nix`, the "qmldir: register this song's
helper types" block). Two other paths into a running shell do **not** write
one:

- `lyra preview` (`pkgs/aoide/crates/lyra/src/commands/preview.rs`)
- `rice stage` hot-sync (`sync_song_widgets`,
  `pkgs/aoide/crates/song/src/widgets.rs`)

Cadenza goes live through hot-sync, so a name-resolved helper would break
there. This is a lyra bug and is routed separately. The kit does not work
around it in core, and it ships **no** `qmldir` in `widgets/`. The build
appends to the qmldir with `>>`, so a shipped qmldir would get every line
twice.

**The mechanism.** Nothing is resolved by type name across files:

1. `import "Kit.js" as Kit` is a relative import of one file (`.pragma
   library`). It needs neither a directory listing nor a qmldir.
2. Each helper is instantiated by URL through a Loader:
   `setSource(kit.helper("Pane"), { …initial props… })`. `helper()` calls
   `Qt.resolvedUrl` inside Kit.js, so it resolves beside Kit.js
   (`qs:@/qs/songs/cadenza/Pane.qml`) wherever the song is mounted.
3. A helper that needs another helper uses the same call (Pane loads
   GlowText by URL).

**The proof** (preview root `$XDG_RUNTIME_DIR/cadenza-foundation`, with no
qmldir anywhere in the song or in the root; `find … -name qmldir` is empty):

- A compile check of every widget file under `qs:` gives `ok` for all nine
  (BarChart, Block, BrailleChart, Gauge, GlowText, Pane, Sparkline,
  Specimen, StateLamp).
- The canvas loads `Specimen.qml`, which instantiates every helper by URL:
  `[aoide/widgetpreview] loaded qs:@/qs/songs/cadenza/Specimen.qml`, and the
  shot is in `design/shots/`.
- Live bindings survive the Loader: a `Qt.binding` passed in `setSource`'s
  initial properties keeps tracking (a pane stat bound to a counter went
  from `13/5` to its next value on the canvas).
- Helper→helper works: the Pane titles in the shot are GlowText loaded by
  Pane.

### The idiom

Every widget builds **one** kit object at its root and passes it down:

```qml
import QtQuick
import "Kit.js" as Kit

Item {
    id: root
    required property var livery
    FontMetrics { id: fm; font: Kit.bodyFont(13) }
    readonly property var kit: Kit.make(root.livery, fm)

    // one nested component, then `Use { helper: "Gauge"; props: {…} }` everywhere
    component Use: Loader {
        required property var kit
        required property string helper
        property var props: ({})
        Component.onCompleted: {
            var p = { kit: Qt.binding(() => kit) }
            for (var k in props) p[k] = props[k]
            setSource(kit.helper(helper), p)
        }
    }

    Use { kit: root.kit; helper: "Pane"; props: ({
        title: "agents", cols: 38, rows: 6, focused: true,
        stat: Qt.binding(() => live + "/" + total),
        content: agentsBody }) }
    Component { id: agentsBody; Column { /* root's ids resolve in here */ } }
}
```

Rules that bit during the build:

- **Always write `kit: root.kit`, never `kit: kit`.** Inside a Loader or
  delegate that declares its own `kit`, the bare name binds to itself and
  stays undefined. An inline `component` cannot see the file's ids, which is
  why `Use` takes `kit` explicitly.
- A value that must stay live goes in as `Qt.binding(() => …)`. A plain
  value is copied once.
- **In a list delegate, call kit functions inline instead of loading
  helpers.** Write a `Text` with `kit.gaugeText(…)` or
  `kit.lampGlyph(s)`/`kit.lampColor(s)` rather than a Loader per row. It is
  one Text per cell, with no per-row component fetch.
- `Kit.js` is **not** on the preview canvas's watch list (it watches `*.qml`)
  and is copied only at build. After editing it, rerun `lyra preview …
  --no-launch` and restart the canvas.

## 2. Kit.js

Module-level exports (for use before a kit exists):

| name | returns |
|---|---|
| `family` | `"JetBrainsMono Nerd Font Mono"` |
| `bodyFont(px)` / `titleFont(px)` | `Qt.font`, regular / bold |
| `helper(name)` | URL of `name.qml` beside Kit.js |
| `withA(color, a)` | the colour at alpha `a` |
| `make(livery, fm)` | **the kit object** (below) |
| `lamps`, `lampGlyph(state)` | the state→glyph map; an unknown state gives `·` |
| `gaugeText(frac, width)` | `{ fill, track }`: the `█`+eighth run and the `░` rest |
| `sparkText(values, width, max)` | right-aligned `▁…█`. `max<=0` auto-scales. A null sample is a space |
| `brailleLines(values, cols, rows, lo, hi)` | `rows` strings, top first, joined vertically so the series reads as a line. `lo`/`hi` null means auto |
| `barColumn(frac, rows)` | `rows` strings, top first: `█`, an eighth cap, blanks |
| `padL(s, n)` / `padR(s, n)` | exactly `n` cells: right-aligned and clipped / left-aligned and clipped with `…` |
| `rep`, `clamp01` | small utilities |
| `glyph` | the icon set (below): named Nerd Font icons for the bar cells |
| `cp(n)` / `cellLen(s)` | a code point as a JS string (a surrogate pair above U+FFFF) / the cells a string takes (code points, not UTF-16 units) |

### The kit object — `Kit.make(livery, fm)`

The binding re-evaluates when the livery or the metrics change, so a palette
swap recolours live. **Colours come only from the livery.** No hex appears
anywhere in the kit.

| role | source | job (intent §2) |
|---|---|---|
| `ground` | base00 | CRT black; the pane fill at `paneAlpha` |
| `raised` | base01 | borderless-block fill |
| `select` | base02 | selected / focused row band |
| `dim` | base03 | axes, labels, rules at rest, timestamps |
| `mid` | base04 | secondary text, idle lamps |
| `ink` | `paletteFg` (base05) | body text, chart ink |
| `bright` | base06 | bright phosphor text |
| `match` | base07 | white-hot: a search match / selected row text |
| `title` | `paletteAccent` (base0B) | titles, the focused rule, working lamps |
| `hot` | `paletteHot` (base0A) | amber: the ONE live element |
| `urgent` | `paletteUrgent` (base08) | red: blocked, failed, awaiting you |
| `warn` | `livery.base09` | orange: past a threshold, `costPartial` |
| `number` | `wireCyan` (base0C) | counts, tokens, durations |
| `path` | `holoBlue` (base0D) | paths, project names, ws numbers |
| `human` | `violet` (base0E) | a person's words |

base00–07 are read from `livery.raw.base16`, because LiveryState has no
per-slot shortcut for them. Each falls back to `paletteBg`/`paletteFg` if
missing.

The kit object also carries:

- **Cell:** `font`, `titleFont`, `cellW` (`advanceWidth("M")`, rounded up to
  0.01px) and `cellH` (`ceil(fm.height)`). Measured at 13px: **7.80 × 18
  px**. Use `cells(n)`/`lines(n)` for pixels and `fit(px)` for whole cells.
  Nothing is hard-coded.
- **Constant:** `paneAlpha` = 0.94.
- **Lamp colour:** `lampColor(state)` maps working→`title`,
  awaiting→`urgent`, failed→`urgent`, stopped→`ink`, idle→`mid`, and
  anything else→`dim`. Awaiting is red because it is "a summons waiting on
  you", and amber stays reserved for the ONE hot element. A widget sets
  `hot` on at most one lamp.
- **Functions:** every glyph builder above, plus `withA`, `helper` and
  `cellLen`, so a helper needs only the object.
- **Icons:** `glyph`, the icon table below.

### The icon set — `kit.glyph`

Icons are Nerd Font glyphs from the one face (intent §2 "Glyphs"): text,
one cell wide, never an image. `Kit.js` holds the only table, so no surface
hard-codes a codepoint. It covers exactly the bar cells that carry an icon;
the clock, `$`, the tray, `⏻` and every list row stay bare text.

| name | codepoint | Nerd Font name | bar cell |
|---|---|---|---|
| `agents` | U+F06A9 | nf-md-robot | AGT (opens OVERVIEW) |
| `cpu` | U+F035B | nf-md-memory | CPU (opens SYS) |
| `notif` | U+F009A | nf-md-bell | the herald count (opens NOTIF) |
| `vol` / `volMuted` | U+F057E / U+F0581 | nf-md-volume_high / volume_off | volume |
| `bt` / `btOff` | U+F00AF / U+F00B2 | nf-md-bluetooth / bluetooth_off | Bluetooth (icon only) |
| `wired` / `wifi` / `netNone` | U+F0200 / U+F05A9 / U+F05AA | nf-md-ethernet / wifi / wifi_off | network (icon only) |
| `battery(level, charging?)` | U+F008E, U+F007A–F0082, U+F0079; U+F0084 | nf-md-battery_outline, battery_10…90, battery; battery_charging | battery: `level` 0..1 picks the tenth |
| `rice` | U+F03D8 | nf-md-palette | RICE (the rice-mode toggle) |

- **Colour.** An icon has no colour job of its own: it takes its value's
  colour (`number`, `warn`, `urgent`, `ink`, or `dim` when the value is
  dim), and `title` while the cell's pane is open or hovered.
- **Width.** The Material glyphs sit above U+FFFF, so a JS string holds each
  as a surrogate pair and `.length` counts 2. Measure mixed text with
  `kit.cellLen(s)`, never `.length`. `padL`/`padR` count UTF-16 units, so
  do not pad a string that holds an icon.
- **Naming.** The kit had no `glyph` member before (`lampGlyph` is a function
  and "glyph builders" is a section of Kit.js), so the table takes the plain
  name and nothing is renamed.
- **One line per cell.** A cell's icon is one `glyph:` line in `bar.qml` plus
  its entry here. Removing an icon means deleting those two lines, and the
  cell then shows its value alone. Bluetooth and network are icon-only, so
  they need a value word back when their icon goes.
- The face carries every codepoint above (fontconfig charset of
  `JetBrainsMonoNerdFontMono-Regular.ttf`: `f0001-f1af0`, `23fb`).

## 3. Helpers

Every helper takes `required property var kit`. All text is
`Text.PlainText`, because a helper may carry untrusted words (house rule 4).

### Pane
| property | default | |
|---|---|---|
| `title` | `""` | cut into the top rule, upper-cased, bold, `kit.title` |
| `stat` | `""` | right-hand stat in the rule |
| `statColor` | `kit.number` | |
| `focused` | false | rule `dim` → `title`, 150ms colour ease |
| `open` | true | false→true: reveal; true→false: close |
| `animateOnCreate` | false | a pane created open skips the reveal unless set. A list delegate must never animate |
| `glow` | `"outline"` | title glow: `off` / `outline` / `bloom` (§5) |
| `innerGlow` | true | `title` phosphor bleeding inward from all four edges (below); `false` opts out |
| `content` | null | a `Component`, instantiated inset one cell and one line, clipped |
| `cols`, `rows` | 20, 3 | INNER size in cells |
| *out* `innerCols`, `innerRows`, `contentItem` | | the inner size when sized from outside; the live body |

Size is `implicitWidth = cells(cols+2)` and `implicitHeight = (rows+1.5)`
lines. The top rule rides the middle of the title line.

Rules are 1px `Rectangle`s. The top rule is three runs (lead-in, title-to-stat,
tail), so the title and stat sit in it with half a cell of air on each side.

Reveal draws the pen clockwise from the top-left corner at constant speed
over **160ms**, linear: each edge gets its share of the perimeter. Then the
fill and content arrive over **60ms**. Close runs pen and fill back
together over **120ms**, InQuad. The fill is `ground` at 0.94. Radius 0.

The inner glow is `title` phosphor bleeding **12px** inward from all four
edges (clamped to half the pane on a small one), fading to transparent
(intent §2 "Inner glow"). It is four static gradient `Rectangle`s drawn
between the fill and the rules, behind the content, and the only gradient
in the song. The top strip is cut into the same three runs as the top rule,
so no light sits under the title or the stat: the light comes from the
rule. The colour is always `title`, whatever the rule's colour, so a pane
at rest is lit too. It starts at **0.14** alpha at rest and **0.26**
focused, with a 150ms ease. It fades in with the fill on reveal and out with
it on close. Nothing animates at rest. Block has no rules and no glow.

### Block
`label`, `stamp` (right-aligned time), `tone` (the label colour, default
`dim`), `selected` (fill `select` instead of `raised`), `cols` (32),
`content: Component`, *out* `contentItem`. There are no rules; the fill is at
`paneAlpha`. Used for one herald toast or one board item.

### GlowText
`text`, `color` (`ink`), `bold`, `glow` (`off` / `outline` / `bloom`),
`bloomAlpha` 0.55, `bloomBlur` 0.5, `bloomMax` 12px.

- `outline` is tier 0: `style: Text.Outline`, `styleColor` = colour at 0.18.
- `bloom` adds tier 1: a hidden source Text feeding one `MultiEffect`
  (blur only, autoPadding).

**Titles only.** The budget is in intent §2.

### Gauge
`value` 0..1, `cells` 16, `label`/`labelCells`, `showPct`, `valueText`,
`color` (`ink`), `warnAt`/`urgentAt` (−1 = off), and `invert` (for "low is
bad", e.g. battery).

The lit colour is urgent if `urgentAt` is passed, otherwise warn if `warnAt`
is passed, otherwise `color`. The track is `dim`, and the percentage is
`number` in 5 cells.

### Sparkline
`values`, `cells` 16, `max` (0 = auto), `label`/`labelCells`, `valueText`
(in `number`), `color`.

### BrailleChart
`values`, `cols` 24, `rows` 4, `lo`/`hi` (null = auto), `axis` (a `dim` hi/lo
axis at the left), `axisCells` 6, `format(v)`, `color`.

### BarChart
`bars: [{label, value, color?}]`, `rows` 5, `barCells` 3, `gapCells` 1,
`max` (0 = auto), `format(v)`, `color`, `labelColor` (`path`). The value
under each bar is in `number` and the label in `labelColor`.

### StateLamp
`status` (not `state`, which is `Item.state`), `hot`. It is a Text: the
glyph comes from `lampGlyph` and the colour is `hot ? kit.hot :
kit.lampColor(status)`.

## 4. Fixtures and what they cannot place

`lyra preview --fixture <set>` copies exactly four files from
`design/fixtures/<set>/` into the preview root's `state/stage/`:
`sessions.json`, `projects.json`, `hooks.json` and `herald.json`
(`FIXTURE_FILES` in `preview.rs`). A missing file gets an empty default.
Nothing else in the set is placed.

| set | placed by `--fixture` | shipped but NOT placed |
|---|---|---|
| `switchboard` | 12 sessions (one sub-agent), 3 projects, hooks, herald (a summons plus 2 toasts with untrusted text) | `graph.json`: live-shaped nodes and edges; 6 workspaces (1 2 3 5 7 9), bindings 2/3→aoide 7→melete 9→mneme, 4 ties (an aoide project clique 2-3-5 and a spawn 2→7) |
| `board` | the same four stage files | `board.json`: §D shape, project aoide, **200 items** (chatter turn/awaiting/joined/left/pingback, mail letter/receipt, herald summons, untrusted text included). `now.json`: §C `by:"workspace"`, 6 rows with null tokens, null cost, `costPartial`, 60-sample series, an unattributed block |

`graph.json` and `state/usage/now.json` have no fixture path, so a widget
that reads them sees whatever the root holds, which is nothing. The root
also carries an empty-stub `state/usage.json` that does not come from the
fixture. **Do not teach a widget a fixture path.** The unplaced files are
the reference shapes that wave 2 builds against. Placing them is a preview
feature request, not a song change.

## 5. Notes and corrections

- **Braille is native.** JetBrainsMono Nerd Font Mono carries U+2800–28FF
  (fontconfig charset 2800). Intent's "falls back per glyph to DejaVu Sans
  Mono" was wrong and has been corrected there.
- **Amber is scarce.** The `[n]` list index that intent §2 draws in amber
  would put many amber things at rest, which conflicts with "never two amber
  things at rest". The specimen draws `[n]` in `path` blue (bar labels) or `dim` (sparkline
  rows), never amber. This is a design
  call for the list surfaces to settle.
- **Glow numbers** are in `intent.md` §2 "Glow — the budget".
