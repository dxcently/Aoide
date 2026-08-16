# Widget Structure — mechanism and hard contracts

**Song:** sonata

How a `widgets/*.qml` file gets from disk to screen, what it is handed, what
its host draws for it, and the contracts it cannot break. Verified against the
five live files (`bar`, `calendar`, `launcher`, `notifications`, `powermenu`)
and the facet chrome they hang off, as they stand today.

Companions: `making-a-widget.md` (what makes a widget belong),
`greek-grammar.md` (the drawing vocabulary), `hazards.md` (live failures).

---

## 1. From file to screen

The full mechanism is `modules/facets/quickshell/qml/slots.md` and
`CONTRACTS.md §5`; this only orients an author.

```
song/songbook/<song>/widgets/<slot>.qml
        │
        ├── nixos-rebuild ──> $out/qml/songs/<song>/<slot>.qml + manifest.json
        │
        └── aoide rice stage <song> ──> ~/Aoide/run/qml/songs/<song>/<slot>.qml
                                        (+ auto quickshell reload when a body changed)
        │
   StagingEngine.resolveSong(activeSong, slot)
        │   active song's own file  →  else SONATA's  →  else ""
        ▼
   WidgetSlot (Item root)          SurfaceSlot (PanelWindow root)
        │ parents + sizes                │ creates, hands back `.item`
        ▼                                ▼
   on screen, inside a host layout   its own layer surface
```

Three consequences worth internalising:

- **sonata IS the floor.** Every other song falls back to these files, so a
  sonata widget is never "just this song's" — it is the shipped default.
- **Being carried is not being rendered.** Nothing appears until a host
  surface embeds an anchor for that exact slot name. Dropping
  `widgets/whatever.qml` with no anchor wired is inert.
- A brand-new slot FILE needs `systemctl --user restart
  aoide-quickshell.service` (the manifest is read once, at startup). An edit
  to an existing file only needs `aoide rice stage`.

---

## 2. What the widget is handed

Every widget declares both of these; both anchors inject them
unconditionally, so omitting one is a load error, not a silent gap:

```qml
required property var notes    // LiveryState singleton — the palette + helpers
required property var bridge   // ShellBridge — the ONLY outbound path from QML
```

Extras are per-slot, not universal (`slots.md` has the authoritative table):

| slot | extras |
|---|---|
| `bar` | `stagingEngine`, `powermenu` (the powermenu slot's live `.item`), `shared` |
| `notifications` | `notification` (the tracked `Notification` model item) |
| `launcher` | `clipboard` (`AoideClipboard`), `ledger` (`GrimoireLedger`) |
| `calendar`, `powermenu` | none beyond the universal pair |

`required` vs optional is a design choice about failure: `bar.qml` declares
`stagingEngine` and `powermenu` as `required property var` (it cannot function
without them) but `shared` as `property var shared: null`, commented
"Optional/null-safe so a standalone bar load never errors." Both choices sit
in the same file, three lines apart.

**No widget ever receives `config.*`.** A song widget is store-copied score,
structurally incapable of reaching nix options through this surface —
`WidgetSlot.qml`, `SurfaceSlot.qml` and `StagingEngine.qml` all restate this.

### What `notes.*` actually exposes

From `modules/facets/quickshell/qml/LiveryState.qml`:

- **colour roles** — `paletteBg/Fg/Accent/Urgent/Hot`, `wireCyan`, `holoBlue`,
  `violet`, `glitchPink`, `base09`, `base0F`; `accentSpread` and
  `noteColor(id)` for one distinct hue per small integer id.
- **component tiers** — `barBg/barFg/barAccent`, `notifBg/notifFg/notifUrgent`,
  `windowBorder/windowBorderInactive`. In sonata the `bar.*` and `notif.*`
  tiers are all-null in the song, so they resolve to the palette values;
  `notifications.qml` reads the `notif*` names anyway (correct — it makes the
  card re-skinnable per song), and `bar.qml` reads the palette names directly.
- **live state** — `songName` (drives slot resolution) and `riceMode`
  (`declarative|staging|draft`, hot from `stage/mode.json`).
- **shared computations** — `ctxBar/ctxPercent/ctxColor/ctxCompact/ctxRollup`
  (context-window gauges), `elapsedSince(iso, nowMs)`, `usageResetIn(...)`,
  `usagePath`. Use these rather than recomputing; they exist so two surfaces
  can't diverge on the same number.

The one-line helper every widget defines for itself (there is no shared
version) is alpha-on-a-role:

```qml
function withA(cstr, a) {
    var c = Qt.color(cstr)
    return Qt.rgba(c.r, c.g, c.b, a)
}
```

---

## 3. Root type follows anchor kind

- `WidgetSlot` is an `Item` that sizes itself off the loaded child's
  `implicitWidth`/`implicitHeight` and parents it into an existing layout →
  **the widget's root must be an `Item`** (`bar.qml`, `calendar.qml`,
  `notifications.qml`).
- `SurfaceSlot` is a non-visual `QtObject` that does no parenting or sizing —
  it creates the widget and hands the instance back as `.item` → **the
  widget's root IS its own `PanelWindow`** (`powermenu.qml`, `launcher.qml`),
  owning its layer, `WlrLayershell.namespace`, keyboard focus and any
  `GlobalShortcut`.

A `PanelWindow`-rooted widget's namespace is **not a free choice**:
`aoide-powermenu` and `aoide-launcher` are what the compositor facet's
layerrules match on for glass, so the string travels with the slot as part of
its contract (catalogued in `slots.md`).

The `bar` slot is the odd one: a `WidgetSlot`-hosted whole-content slot whose
`PanelWindow` stays in the facet. The song owns its own footprint —
`shell.qml` binds the window's `implicitHeight` AND `exclusiveZone` to
`barSlot.implicitHeight` — and the widget binds `width: parent ? parent.width
: implicitWidth`, because under `Component.createObject(root, props)` its
`parent` IS the `WidgetSlot`, which `shell.qml` anchors to fill the window.

---

## 4. Choosing your host chrome — free vs. must-supply

| host | you get for free | you must supply |
|---|---|---|
| **`StelePopout`** (bar cell popout, the default) | a real `PopupWindow` (xdg_popup of the bar) anchored under `cell`, a 4px gap under the strip, sizing from your `width` + `implicitHeight` (+6 / +8 for shadow overhang), optional `anchorEdges`/`anchorGravity` left-pinning | **all** chrome: cast shadow, body fill, 2px border, inset keyline, crown/name/tag, frieze, frames, ledger. You set `width` and `implicitHeight` |
| **`SteleLayerPopout`** | everything `StelePopout` gives, but on its OWN wlr-layershell surface + namespace (default `aoide-calendar`), left-pinned by construction, re-anchored from the cell's scene position at each open | same as `StelePopout` |
| **`BarPopout`** | a `PopupWindow` PLUS `GadgetFrame` chrome — pediment with the Greek order-mark, cornice, `║` column rails, `▔▁` stylobate, the `title` inscribed on the architrave, glass fill at `paletteBg` 0.72, an opt-in `reveal` unroll | only the body content (`width: parent.width`), and its colours |
| **`WidgetSlot` in a host layout** (notification stack, bar's calendar popout) | parenting, sizing from your implicit size, prop injection | root `Item`, your own footprint, all chrome |
| **`SurfaceSlot`** | object creation with props; `.item` for the host to call | your own `PanelWindow`, layer, namespace, `keyboardFocus`, `visible` gating, scrim, dismissal, shortcuts |

**Standing direction (khoa, 2026-07-31), stated in `StelePopout.qml`'s own
header:** new bar widgets converge their outer panel on the stele family,
hosted BARE through `StelePopout` — not `GadgetFrame`. Live recon on the
calendar confirmed genuine double-framing under `GadgetFrame` (two captions,
two closures, glass losing the numerals over dark windows). `BarPopout` is
what the battery gauge still uses; it is not the default for something new.

`SteleLayerPopout` exists for exactly one reason: Hyprland's `blur_popups`
layerrule frosts EVERY xdg_popup of the matched layer, and an xdg_popup has no
namespace of its own, so a single popout cannot opt out. Reach for it only
when your surface must diverge from the bar layer's compositor rules.

**The `+5` convention.** A self-framed stele's cast shadow overhangs 4px right
and 5px bottom, so its root reports `implicitHeight: stele.height + 5` and the
host adds `+6`/`+8` around it. Skipping the `+5` clips your own shadow.

---

## 5. Colour discipline

Every colour reads from `notes.*` or a same-named local alias
(`readonly property color ink: notes.paletteFg`, `clay: notes.base09`,
`aegean: notes.holoBlue`, `sig: notes.base0F`). What actually exists outside
that rule today, exhaustively:

**Sanctioned material literals** — light and shadow, not palette. The Aero
gloss gradient's four white stops — `0.13 → 0.03 → 0.00 → 0.03` on
`powermenu.qml`'s steles and `launcher.qml`'s pages, `0.14` at the top stop on
`launcher.qml`'s incantation strip and `GadgetFrame.qml` — the page-turn shadows
`Qt.rgba(0,0,0,0.4)` (`launcher.qml`), and the white outline on the bar's
selected note glyph `styleColor: Qt.rgba(1,1,1,0.9)` (labeled in `bar.qml`'s
header as "one sanctioned literal").

**Two labeled exceptions**, each argued in its own file's header:

- `bar.qml`'s **structural ink is literal `#000000`** — staff lines, barlines,
  playhead, page rail. The manuscript metaphor treats printed staff lines as
  ink-on-paper, not a themed chrome role. Text ink on the strip is still
  `notes.paletteFg`.
- `calendar.qml`'s **moon-phase glyphs render as real colour emoji**
  (`U+1F311`–`U+1F318`) — "the ONE glyph family wearing fixed color outside
  the `notes.*` roles", with the `U+FE0E` monochrome fallback recorded as the
  rollback path.

**One unlabeled leftover, not a precedent.** `bar.qml`'s battery `BarPopout`
body draws its two `Text` elements in literal `"#14141a"` (lines 1183, 1191) —
a survival from the old dark popout palette, covered by no header note. It is
the only bare hex in ordinary widget chrome anywhere in the five files. Do not
copy it; a new popout body inks from `notes.*`.

A widget breaking the `notes.*` rule **without** a header note like the two
above is the anomaly worth flagging in review.

---

## 6. Geometry constants

- **`radius: 0` on every panel, card, cell, page and popout body.** Matches
  the compositor facet's own window rounding, which defaults to 0
  (`modules/facets/compositor/default.nix`, plus an explicit
  `windowrule = rounding 0, match:class kitty`). Two deliberate
  exceptions exist, both in `bar.qml`: the workspace state marks are true
  circles (`radius: width / 2`, with equal width and height so both axes round
  fully), and the drawn barlines carry `radius: 0.5` on a 1.5–3px rule
  (hairline softening, not a rounded corner). A rounded card corner anywhere
  would be the one visibly foreign element on this desktop.
- **2px `paletteFg` border** on every stele body — `notifications.qml`'s card,
  `powermenu.qml`'s `EndingStele`, `calendar.qml`'s sheet, `launcher.qml`'s
  `GlassPage`, `BoardSegment` and incantation strip, `AudioColonnade`'s stele.
  The one geometry constant every "stele" shares regardless of order.
- **Inset keyline at `margins: 4`** (powermenu uses 5 on its larger bay), 1px,
  in the signature hue.
- **Cast shadow: `+4` right / `+5` down at ink `0.22`.** Drawn as one offset
  `Rectangle` normally; `calendar.qml` draws the same shadow as two strips
  (right + bottom) at identical alpha and identical offsets — originally
  because an underlapping rect would have tinted the transparent window its
  sheet then cut over the day grid. That window has since been reverted to an
  opaque fill; the strips stayed, since their union is pixel-identical around
  an opaque sheet. Two files, months apart, landing on the same two numbers is
  the idiom.
- **Content `Column`: `anchors.margins: 10–12`, `spacing: 4`.**
- **The gloss sheen marks GLASS, not paper.** The four-stop white gradient
  rides `powermenu.qml`'s steles and `launcher.qml`'s pages — both
  compositor-blurred `PanelWindow` overlays. It is absent from
  `notifications.qml` and `calendar.qml` (opaque "paper" reads, bare stele
  hosting) and was removed outright from the bar. Do not add sheen to an
  opaque surface.

---

## 7. Layout idioms (and the one that is banned)

Plain `Column` / `Row` / `Item` with explicit `width: parent.width` and
anchor-between-siblings for flexible text. That is the idiom throughout
`GadgetFrame`, `AoidePanel`, `PowerVitalsGadget` and every song widget.

**`QtQuick.Layouts` is not used by any widget.** `notifications.qml`'s header
records why: a v1 card hit a real Qt Quick Layouts sizing-negotiation loop
("RangeError: Maximum call stack size exceeded", live in journalctl) the
moment wrapped body text rendered — nested `Layout` items under an ancestor
whose `implicitHeight` derives from the layout's own `implicitHeight`.
`bar.qml` still carries an `import QtQuick.Layouts` line but uses no type from
it.

Recurring sizing patterns worth copying:

- Size a container off a computed constant, not off its children, when the
  children also depend on it (`calendar.qml`'s memento row uses
  `width: 3 * 124 + 2 * 8` with the comment "not parent.width (binding loop:
  the Column sizes from us)").
- A filler/overlay that must not affect sizing is a SIBLING of the
  `ListView`, never its footer — "a footer counts toward contentHeight and
  sizing off it loops" (`launcher.qml`).
- Declare a full-surface click-catcher BEFORE the interactive children so
  nested `MouseArea`s win the hit test (`notifications.qml`'s
  click-anywhere-dismiss).

---

## 8. Helper components: nested, not sibling files

`slots.md` documents a sibling-file mechanism (an UPPERCASE filename carried
by the build, never resolvable as a slot). **No widget uses it.** Every helper
today is a `component Name: BaseType { … }` block nested inside the file:
`bar.qml`'s `WorkspaceRow` and `Barline`, `calendar.qml`'s `Roller`,
`powermenu.qml`'s `EndingStele`, `launcher.qml`'s `ManuscriptRow`, `BookPage`,
`GlassPage`, `PageStack`, `BoardSegment`.

The reason is load-bearing: Quickshell's dynamic `Component.createObject(url)`
loading — how both anchors load every widget — does not reliably grant a
loaded file visibility into custom types in its own directory. Confirmed live:
both the implicit same-directory rule and a top-level file-scoped `component`
failed ("WorkspaceRow is not a type" / "Syntax error"). Nesting sidesteps
cross-file resolution entirely. Nesting depth is free — `launcher.qml`
declares `GlassPage`/`PageStack`/`BoardSegment` inside its `book` Item, not at
the root.

---

## 9. Motion tiers

Durations and easings actually in use. Stay inside these bands; a new widget
that animates at 900ms where the house animates at 160 reads as a different
program.

| band | duration / easing | used for |
|---|---|---|
| micro | 140–180ms, `OutCubic` (or bare `ColorAnimation`) | `Behavior on color` 150/160; glyph size 140; highlight/ring travel 150/180; meter fill 140 |
| hover pop | 170ms `OutBack` (scale) + 170ms `OutCubic` (lift) | powermenu stele |
| reveal / page | 200–260ms `OutCubic` | `BarPopout.reveal` 200; calendar page-slide 220, unfurl 260 |
| morph / summon | 300–340ms | launcher `fold` 300 `OutCubic`; calendar mode morph 340 `InOutCubic` |
| leaf turn | 120ms `InQuad` out + 140ms `OutQuad` in | launcher chapter flip |
| staged deal | 560ms open / 200ms close, LINEAR master clock, per-item window `stag = 0.085` with `OutCubic` inside the window | powermenu |
| attention pulse | 600ms × 2 `InOutQuad` (0.35↔1.0); 700ms × 2 for a card (0.8↔1.0) | urgent workspace, blocked sessions cell, critical notification |
| flourish | 450–1500ms, looping while hovered, reset in `onStopped` | powermenu per-glyph |

Rules: a looping animation resets its property in `onStopped`; a pulse that
can end mid-cycle uses `alwaysRunToEnd: true` so it settles opaque; motion
that must not disturb layout rides `transform`, not anchors.

---

## 10. The file header

Every widget opens with a substantial `//` block before its imports. Across
the five, it carries — not every widget needs every element:

- **Identity** — filename, slot name, one line on what it is.
- **Order breakdown** — ORDER / SIGNATURE / CROWN / FRIEZE in a few lines
  each. Decisions, not arguments (`making-a-widget.md` §6).
- **Wiring** — anchor kind, extras, namespace; cross-reference `slots.md` and
  `CONTRACTS.md §5` rather than restating them.
- **A dated iteration log** — `calendar.qml` carries six same-day rounds. This
  header is the widget's design record; there is no shared per-widget map
  anywhere else.
- **Grounding citations** — a claim about engine behaviour is backed by
  something checked: `notifications.qml` cites the compiled
  `quickshell-service-notifications.qmltypes`, and traces the exact
  `server.cpp`/`notification.cpp` path that made an earlier `tracked = false`
  write actively harmful.
- **A "why" for anything a first reader would question** — the nested-component
  rule, the sanctioned literal, the alpha cap on a hue.

**Section banners, two tiers.** Single-line `// ── label ───…` (U+2500) marks
subsections; every file uses it, 11 to 24 times. Heavier `// ══ LABEL ═══…`
(U+2550) marks top-level zones and is used by four of five (`bar.qml` 7,
`launcher.qml` 4, `calendar.qml` 2, `powermenu.qml` 2). `notifications.qml`
uses only the single tier — it is also the flattest file, one `Column` of
parts with no competing zones. Scaling down is fine; inventing a third tier
is not.

---

## 11. The glass is compositor-side

The frosted depth that makes a popout read as a framed stele is NOT drawable
in QML (`modules/facets/compositor/default.nix`):

- `layerrule = blur on` + `ignore_alpha 0.05` per namespace (`aoide-dock`,
  `aoide-launcher`, `aoide-powermenu`).
- `layerrule = blur_popups on, match:namespace aoide-bar` extends the blur to
  the bar's `xdg_popup` children — this is what frosts every `BarPopout`- and
  `StelePopout`-hosted card without either widget touching a compositor
  option.
- `plugin:hyprglass` layers refraction/fresnel over the flat blur for the same
  namespace set.
- A widget opts OUT only by taking its own layer namespace
  (`SteleLayerPopout` → `aoide-calendar`, matched by `blur off`).

A widget earns the frosted read by which host anchor and namespace it hangs
under, not by anything it paints. Changing how glassy a popout feels is a
facet edit (out of a song widget's scope) or a hosting choice — never a blur
effect in QML.
